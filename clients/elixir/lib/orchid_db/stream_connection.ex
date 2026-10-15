defmodule OrchidDB.StreamConnection do
  @moduledoc """
  Kafka execution with caller-owned authorization. The required authorizer takes
  `(subject, checks)` and returns one boolean per check. Use your application's
  SpiceDB client here; caching and expiry remain application-owned.

  Results carry base64 Arrow IPC. Persist subscription offsets only after every
  replacement frame in the batch is processed. One operation runs per process.
  """
  use GenServer
  alias OrchidDB.Internal.Runtime

  def start_link(schema, options, authorizer, opts \\ []) when is_function(authorizer, 2),
    do: GenServer.start_link(__MODULE__, {schema, options, authorizer, opts})

  def insert(server, change, subject),
    do: run(server, "insert", %{change: change, subject: subject})

  def insert_batch(server, changes, subject),
    do: run(server, "insert_batch", %{changes: changes, subject: subject})

  def query(server, text, subject, parameters \\ %{}),
    do: run(server, "query", %{text: text, subject: subject, parameters: parameters})

  def query_hybrid(server, text, subject, snapshot, parameters \\ %{}),
    do:
      run(server, "hybrid", %{
        text: text,
        subject: subject,
        parameters: parameters,
        snapshot: snapshot
      })

  def subscribe(server, text, subject, offsets, parameters \\ %{}),
    do:
      run(server, "subscribe", %{
        text: text,
        subject: subject,
        offsets: offsets,
        parameters: parameters
      })

  def next_batch(server, max_records \\ 1, wait_ms \\ 100),
    do: run(server, "next", %{max_records: max_records, wait_ms: wait_ms})

  def unsubscribe(server), do: run(server, "unsubscribe", %{})

  def close(server) do
    # Cancel native work without waiting behind a long-running GenServer call.
    case GenServer.whereis(server) do
      nil ->
        :ok

      pid ->
        case Process.info(pid, :dictionary) do
          {:dictionary, entries} ->
            case List.keyfind(entries, {__MODULE__, :handle}, 0) do
              {_, {id, opts}} -> Runtime.compile(%{op: "stream_close", id: id}, opts)
              nil -> :ok
            end

          nil ->
            :ok
        end

        GenServer.cast(pid, :close)
    end
  end

  defp run(server, action, values), do: GenServer.call(server, {:run, action, values}, :infinity)

  @impl true
  def init({schema, options, authorizer, opts}) do
    request =
      options
      |> Map.new(fn {k, v} -> {to_string(k), v} end)
      |> Map.merge(%{"op" => "stream_open", "schema" => schema})

    case Runtime.compile(request, opts) do
      {:ok, %{"id" => id}} ->
        Process.put({__MODULE__, :handle}, {id, opts})
        {:ok, %{id: id, authorizer: authorizer, opts: opts}}

      {:error, reason} ->
        {:stop, reason}
    end
  end

  defp command(state, op, values \\ %{}),
    do: Runtime.compile(Map.merge(values, %{op: op, id: state.id}), state.opts)

  @impl true
  def handle_call({:run, action, values}, _from, state) do
    try do
      result =
        with {:ok, _} <- command(state, "stream_start", Map.put(values, :action, action)),
             do: poll(state)

      {:reply, result, state}
    rescue
      error -> {:stop, :normal, {:error, Exception.message(error)}, state}
    catch
      kind, _reason -> {:stop, :normal, {:error, "authorization callback failed: #{kind}"}, state}
    end
  end

  @impl true
  def handle_cast(:close, state), do: {:stop, :normal, state}

  defp poll(state) do
    case command(state, "stream_poll") do
      {:ok, %{"state" => "done", "result" => result}} ->
        {:ok, result}

      {:ok, %{"state" => "pending"}} ->
        poll(state)

      {:ok, %{"state" => "authorize"} = event} ->
        decisions = state.authorizer.(event["subject"], event["checks"])

        unless is_list(decisions) and length(decisions) == length(event["checks"]) and
                 Enum.all?(decisions, &is_boolean/1),
               do: raise(ArgumentError, "authorizer must return one boolean per check")

        case command(state, "stream_authorize", %{nonce: event["nonce"], decisions: decisions}) do
          {:ok, _} -> poll(state)
          {:error, _} = error -> raise(RuntimeError, inspect(error))
        end

      error ->
        error
    end
  end

  @impl true
  def terminate(_reason, state) do
    command(state, "stream_close")
    :ok
  end
end
