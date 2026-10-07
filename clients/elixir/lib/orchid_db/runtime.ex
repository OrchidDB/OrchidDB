defmodule OrchidDB.Internal.Runtime do
  @moduledoc false
  @external_resource Path.expand("../../CORE_REVISION", __DIR__)
  @core_revision File.read!(Path.expand("../../CORE_REVISION", __DIR__)) |> String.trim()
  @doc "Compile a v1 request map to SQL. Requires ORCHIDDB_NATIVE_LIBRARY or :library."
  def compile(request, opts \\ []) when is_map(request) do
    case Keyword.get(opts, :statistics) do
      nil -> compile_without_statistics(request, opts)
      statistics -> OrchidDB.Internal.Statistics.compile(statistics, request)
    end
  end

  defp compile_without_statistics(request, opts) do
    path = Keyword.get(opts, :library) || System.get_env("ORCHIDDB_NATIVE_LIBRARY")

    if is_nil(path) do
      {:error, "Set ORCHIDDB_NATIVE_LIBRARY to the compiler shared library"}
    else
      with {:ok, json} <- Jason.encode(request),
           {:ok, response, revision} <- OrchidDB.Native.compile_json(to_string(path), json),
           :ok <- check_revision(revision),
           {:ok, decoded} <- Jason.decode(response) do
        case decoded do
          %{"ok" => true, "result" => result} -> {:ok, result}
          %{"ok" => false, "error" => error} -> {:error, error}
          _ -> {:error, "Unsupported compiler response"}
        end
      end
    end
  end

  @doc "Shared statistics protocol for application-owned bounded sessions."
  def statistics_command(command, opts \\ []) do
    path = Keyword.get(opts, :library) || System.get_env("ORCHIDDB_NATIVE_LIBRARY")

    if is_nil(path) do
      {:error, "Set ORCHIDDB_NATIVE_LIBRARY to the compiler shared library"}
    else
      with {:ok, json} <- Jason.encode(command),
           {:ok, response, revision} <- OrchidDB.Native.statistics_json(to_string(path), json),
           :ok <- check_revision(revision),
           {:ok, decoded} <- Jason.decode(response) do
        case decoded do
          %{"ok" => true, "result" => result} -> {:ok, result}
          %{"ok" => false, "error" => error} -> {:error, error}
          _ -> {:error, "Unsupported statistics response"}
        end
      end
    end
  end

  @doc false
  def remote_command(command, opts \\ []) do
    path = Keyword.get(opts, :library) || System.get_env("ORCHIDDB_NATIVE_LIBRARY")

    if is_nil(path) do
      {:error, "Set ORCHIDDB_NATIVE_LIBRARY to the compiler shared library"}
    else
      with {:ok, json} <- Jason.encode(command),
           {:ok, response, revision} <- OrchidDB.Native.remote_json(to_string(path), json),
           :ok <- check_revision(revision),
           {:ok, decoded} <- Jason.decode(response) do
        case decoded do
          %{"ok" => true, "result" => result} -> {:ok, result}
          %{"ok" => false, "error" => error} -> {:error, error}
          _ -> {:error, "Unsupported remote engine response"}
        end
      end
    end
  end

  defp check_revision(@core_revision), do: :ok
  defp check_revision(revision) when revision == @core_revision <> "-dirty", do: :ok
  defp check_revision(_), do: {:error, "Compiler core revision does not match this client"}

  @doc "Borrow a connection. The Arrow stream must be consumed inside callback."
  def query_arrow(connection, request, callback, opts \\ []) when is_function(callback, 1) do
    with {:ok, plan} <- compile(request, opts) do
      if Map.get(plan, "transfers", []) != [] do
        {:error, "Use query_federated for a multi-engine plan"}
      else
        Adbc.Connection.query_pointer(connection, plan["sql"], callback)
      end
    end
  end

  @doc """
  Execute engine operations on named caller-owned sessions. SQL registry entries
  have `:dialect` and `:query` (SQL, callback). Request engines have `:dialect` and
  `:execute_requests` (bound requests, declared columns), returning `{:ok, data}`
  or data as `%{rows: typed_rows}` / `%{ipc: base64_arrow_stream}`.

  SQL callbacks provide ADBC streams or typed row arrays. An optional
  `:exchange_data` converts driver results to rows or IPC. Connections remain
  caller-owned; no tables or views are created.
  """
  def query_federated(engines, request, callback, opts \\ []) do
    with {:ok, plan} <- compile(request, opts) do
      execute_federated(engines, plan, callback, opts)
    end
  end

  @doc "Execute an already compiled plan with caller-owned sessions."
  def execute_federated(engines, plan, callback, opts \\ []) do
    target_id = plan["execution_engine"] || raise ArgumentError, "Missing execution_engine"
    transfers = Map.get(plan, "transfers", [])

    routes = [
      {target_id, plan["dialect"]}
      | Enum.flat_map(transfers, fn transfer ->
          source = {transfer["source_engine"], transfer["source_dialect"]}

          case operation(transfer) do
            nil -> [source]
            {"request", op} -> [source, {op["engine"], op["template"]["adapter"]}]
            {"operation", op} -> [source, {op["engine"], op["template"]["dialect"]}]
          end
        end)
    ]

    Enum.each(routes, fn {id, dialect} ->
      if !Map.has_key?(engines, id) or engines[id].dialect != dialect,
        do: raise(ArgumentError, "Missing engine or dialect mismatch: #{id}")
    end)

    bound = execute_transfers(plan, engines, opts)
    engines[target_id].query.(bound["sql"], callback)
  end

  defp operation(transfer) do
    cond do
      is_map(transfer["request"]) -> {"request", transfer["request"]}
      is_map(transfer["operation"]) -> {"operation", transfer["operation"]}
      true -> nil
    end
  end

  defp execute_transfers(%{"transfers" => []} = plan, _engines, _opts), do: plan

  defp execute_transfers(plan, engines, opts) do
    # Each binding rewrites pending source SQL; never iterate a stale transfer copy.
    [transfer | _] = plan["transfers"]
    relation = transfer["target_relation"]

    current =
      case operation(transfer) do
        nil ->
          with_source(engines[transfer["source_engine"]], transfer["sql"], fn data ->
            compiler_command!(
              Map.merge(data, %{op: "bind", plan: plan, relation: relation}),
              opts
            )
          end)

        {_kind, _operation} ->
          bind_inputs = fn data ->
            compiler_command!(
              Map.merge(data, %{op: "bind_operation", plan: plan, relation: relation}),
              opts
            )
          end

          prepared =
            if transfer["sql"] == "" do
              bind_inputs.(%{rows: [[]]})
            else
              with_source(engines[transfer["source_engine"]], transfer["sql"], bind_inputs)
            end

          if Map.has_key?(prepared, "requests") do
            engine = engines[prepared["engine"]]

            data =
              engine.execute_requests.(prepared["requests"], prepared["columns"])
              |> unwrap_source_result()

            compiler_command!(
              Map.merge(data, %{op: "bind", plan: plan, relation: relation}),
              opts
            )
          else
            batches =
              Enum.map(prepared["sql"], fn sql ->
                with_source(engines[prepared["engine"]], sql, & &1)
              end)

            compiler_command!(
              %{op: "bind", plan: plan, relation: relation, batches: batches},
              opts
            )
          end
      end

    execute_transfers(current, engines, opts)
  end

  defp with_source(source, sql, consume) do
    source.query.(sql, fn stream ->
      data =
        cond do
          Map.has_key?(source, :exchange_data) ->
            source.exchange_data.(stream)

          is_struct(stream, Adbc.StreamResult) ->
            %{ipc: Base.encode64(Adbc.StreamResult.to_ipc_stream(stream))}

          true ->
            %{rows: stream}
        end

      consume.(data)
    end)
    |> unwrap_source_result()
  end

  defp compiler_command!(command, opts) do
    case compile_without_statistics(command, opts) do
      {:ok, result} -> result
      {:error, error} -> raise ArgumentError, error
    end
  end

  defp unwrap_source_result({:ok, plan}), do: plan
  defp unwrap_source_result({:error, error}), do: raise(ArgumentError, inspect(error))
  defp unwrap_source_result(result) when is_map(result) or is_list(result), do: result
end
