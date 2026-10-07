defmodule OrchidDB.Statistics do
  @moduledoc """
  Immutable retained statistics. Generation calls the shared Rust coordinator.
  The collector receives SQL, dialect, max_rows, max_bytes and timeout_ms; it
  must enforce these bounds using the caller-owned session, returning
  `{:ok, %{"rows" => rows}}` or `{:ok, %{"ipc" => base64_arrow_stream}}`.
  Include `"truncated" => true` if a transport cap stops before EOF.
  Return `{:error, reason}` if bounded execution is unsupported. Such sources
  appear in the generation report. No database connection is opened here.
  """
  defstruct [:catalog_id, :snapshot, :report, opts: []]

  def generate(request, collect, opts \\ []) when is_function(collect, 1) do
    with {:ok, state} <- OrchidDB.statistics_command(%{op: "begin", request: request}, opts) do
      try do
        case collect_all(state, collect, opts) do
          {:ok, result} ->
            {:ok,
             %__MODULE__{
               catalog_id: result["catalog_id"],
               snapshot: result["snapshot"],
               report: result["report"],
               opts: opts
             }}

          {:error, _} = error ->
            OrchidDB.statistics_command(%{op: "cancel", id: state["id"]}, opts)
            error
        end
      rescue
        error ->
          OrchidDB.statistics_command(%{op: "cancel", id: state["id"]}, opts)
          reraise error, __STACKTRACE__
      end
    end
  end

  defp collect_all(%{"id" => id, "request" => nil}, _collect, opts),
    do: OrchidDB.statistics_command(%{op: "finish", id: id}, opts)

  defp collect_all(%{"id" => id, "request" => request}, collect, opts) do
    response =
      case collect.(request) do
        {:ok, response} when is_map(response) -> response
        {:error, reason} -> %{"error" => if(is_binary(reason), do: reason, else: inspect(reason))}
      end

    submission =
      Map.merge(response, %{"op" => "submit", "id" => id, "request_id" => request["id"]})

    with {:ok, state} <- OrchidDB.statistics_command(submission, opts),
         do: collect_all(state, collect, opts)
  end

  @doc "Compile using this retained catalog, preserving all plan diagnostics."
  def compile(%__MODULE__{} = statistics, request),
    do:
      OrchidDB.statistics_command(
        %{op: "compile", catalog_id: statistics.catalog_id, request: request},
        statistics.opts
      )

  @doc "Save a portable JSON snapshot; no native handles are persisted."
  def save(%__MODULE__{snapshot: snapshot}, path) do
    with {:ok, json} <- Jason.encode(snapshot), do: File.write(path, json)
  end

  def install(snapshot, opts \\ []) do
    with {:ok, installed} <-
           OrchidDB.statistics_command(%{op: "install", snapshot: snapshot}, opts) do
      {:ok, %__MODULE__{catalog_id: installed["catalog_id"], snapshot: snapshot, opts: opts}}
    end
  end

  def load(path, opts \\ []) do
    with {:ok, json} <- File.read(path),
         {:ok, snapshot} <- Jason.decode(json),
         do: install(snapshot, opts)
  end

  @doc "Replace a catalog only after generation succeeds."
  def regenerate(%__MODULE__{} = previous, request, collect) do
    with {:ok, replacement} <- generate(request, collect, previous.opts) do
      clear(previous)
      {:ok, replacement}
    end
  end

  @doc "Release the native catalog after its last user finishes."
  def clear(%__MODULE__{} = statistics),
    do:
      OrchidDB.statistics_command(
        %{op: "release", catalog_id: statistics.catalog_id},
        statistics.opts
      )
end
