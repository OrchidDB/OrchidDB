defmodule OrchidDB.RemoteEngine do
  @moduledoc """
  Caller-owned Quickwit, Elasticsearch, or Weaviate session using the shared native HTTP
  transport. Pass `registry/1` to `OrchidDB.query_federated/4`. Stop the process or
  call `close/1` when finished; SQL connections are never owned by this process.
  """
  use GenServer

  def start_link(adapter, options, opts \\ []) when adapter in ["quickwit", "elasticsearch", "weaviate"] do
    GenServer.start_link(__MODULE__, {adapter, options, opts})
  end

  def registry(server) do
    %{
      dialect: GenServer.call(server, :adapter),
      execute_requests: fn requests, columns ->
        execute_requests(server, requests, columns)
      end
    }
  end

  def execute_requests(server, requests, columns),
    do: GenServer.call(server, {:execute, requests, columns}, :infinity)

  def clear_metadata_cache(server), do: GenServer.call(server, :clear_metadata_cache, :infinity)
  def close(server), do: GenServer.call(server, :close, :infinity)

  @impl true
  def init({adapter, options, opts}) do
    case OrchidDB.Internal.Runtime.remote_command(%{op: "open", adapter: adapter, options: options}, opts) do
      {:ok, %{"id" => id}} -> {:ok, %{id: id, adapter: adapter, opts: opts}}
      {:error, reason} -> {:stop, reason}
    end
  end

  @impl true
  def handle_call(:adapter, _from, state), do: {:reply, state.adapter, state}
  def handle_call(:close, _from, %{id: nil} = state), do: {:reply, :ok, state}

  def handle_call(:close, _from, state) do
    case OrchidDB.Internal.Runtime.remote_command(%{op: "close", id: state.id}, state.opts) do
      {:ok, _} -> {:reply, :ok, %{state | id: nil}}
      error -> {:reply, error, state}
    end
  end

  def handle_call(_command, _from, %{id: nil} = state),
    do: {:reply, {:error, "Remote engine is closed"}, state}

  def handle_call(:clear_metadata_cache, _from, state) do
    result = OrchidDB.Internal.Runtime.remote_command(%{op: "clear_metadata_cache", id: state.id}, state.opts)
    {:reply, result, state}
  end

  def handle_call({:execute, requests, columns}, _from, state) do
    result =
      OrchidDB.Internal.Runtime.remote_command(
        %{op: "execute", id: state.id, requests: requests, columns: columns, format: "ipc"},
        state.opts
      )

    {:reply, result, state}
  end

  @impl true
  def terminate(_reason, %{id: nil}), do: :ok

  def terminate(_reason, state) do
    OrchidDB.Internal.Runtime.remote_command(%{op: "close", id: state.id}, state.opts)
    :ok
  end
end
