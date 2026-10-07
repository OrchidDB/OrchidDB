defmodule OrchidDB.Connection do
  @moduledoc "Schema registered on a caller-owned database connection."
  @enforce_keys [:engine, :schema, :dialect]
  defstruct [:engine, :schema, :dialect, opts: []]
end

defmodule OrchidDB do
  @moduledoc "Execute graph query text separately from the registered schema."
  alias OrchidDB.Internal.Runtime

  def connect(engine, schema, opts \\ []) when is_map(schema) do
    with {:ok, schema} <-
           Runtime.compile(
             %{op: "validate_schema", schema: schema},
             Keyword.delete(opts, :statistics)
           ) do
      {:ok,
       %OrchidDB.Connection{
         engine: engine,
         schema: schema,
         dialect: Keyword.get(opts, :dialect, "duckdb"),
         opts: opts
       }}
    end
  end

  defp request(connection, text, opts) when is_binary(text) do
    Map.merge(connection.schema, %{
      "version" => 1,
      "dialect" => connection.dialect,
      "language" => Keyword.get(opts, :language, "cypher"),
      "query" => text,
      "parameters" => Keyword.get(opts, :parameters, %{}),
      "authorization" => Keyword.get(opts, :authorization)
    })
  end

  def query(connection, text, opts \\ []) when is_binary(text) do
    with {:ok, work} <- Runtime.compile(request(connection, text, opts), connection.opts) do
      if Map.get(work, "transfers", []) != [] do
        {:error, "Use query_federated on this connection for multiple engines"}
      else
        Adbc.Connection.query(connection.engine, work["sql"])
      end
    end
  end

  def query_arrow(connection, text, callback, opts \\ []) when is_binary(text) do
    Runtime.query_arrow(
      connection.engine,
      request(connection, text, opts),
      callback,
      connection.opts
    )
  end

  def query_federated(connection, text, engines, callback, opts \\ []) when is_binary(text) do
    Runtime.query_federated(engines, request(connection, text, opts), callback, connection.opts)
  end

  def generate_statistics(connection, collector) do
    with {:ok, statistics} <-
           OrchidDB.Internal.Statistics.generate(
             request(connection, "RETURN 1", []),
             collector,
             connection.opts
           ) do
      close(connection)
      {:ok, %{connection | opts: Keyword.put(connection.opts, :statistics, statistics)}}
    end
  end

  def close(connection) do
    case Keyword.get(connection.opts, :statistics) do
      nil -> :ok
      statistics -> OrchidDB.Internal.Statistics.clear(statistics)
    end
  end

  def clear_statistics(connection) do
    close(connection)
    %{connection | opts: Keyword.delete(connection.opts, :statistics)}
  end

  def save_statistics(connection, path) do
    case Keyword.get(connection.opts, :statistics) do
      nil -> {:error, "No statistics have been generated"}
      statistics -> OrchidDB.Internal.Statistics.save(statistics, path)
    end
  end

  def load_statistics(connection, path) do
    with {:ok, statistics} <-
           OrchidDB.Internal.Statistics.load(path, Keyword.delete(connection.opts, :statistics)) do
      close(connection)
      {:ok, %{connection | opts: Keyword.put(connection.opts, :statistics, statistics)}}
    end
  end
end
