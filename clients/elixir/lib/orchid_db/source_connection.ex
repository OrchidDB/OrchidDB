defmodule OrchidDB.SourceConnection do
  @moduledoc "Source-first graph execution with application-owned SQL callbacks and local functions."
  alias OrchidDB.Internal.Runtime
  defstruct [:schema, :engine, :sources, :functions, :completion, :opts]

  @doc "Sources provide :dialect and :query (SQL to a resource-safe row Enumerable)."
  def new(schema, engine, sources, opts \\ []) when engine in [:none, :duckdb] do
    with {:ok, validated} <- Runtime.compile(%{op: "validate_schema", schema: schema}, opts) do
      {:ok,
       %__MODULE__{
         schema: validated,
         engine: engine,
         sources: sources,
         functions: Keyword.get(opts, :functions, %{}),
         completion: Keyword.get(opts, :completion),
         opts: opts
       }}
    end
  end

  @doc "Return a lazy Enumerable of positional rows; source callbacks own resource cleanup."
  def query(connection, text, opts \\ []) do
    request =
      Map.merge(connection.schema, %{
        "version" => 1,
        "dialect" => "duckdb",
        "engine" => Atom.to_string(connection.engine),
        "language" => Keyword.get(opts, :language, "cypher"),
        "query" => text,
        "parameters" => Keyword.get(opts, :parameters, %{}),
        "authorization" => Keyword.get(opts, :authorization)
      })

    with {:ok, %{"version" => 2, "execution_plan" => plan}} <-
           Runtime.compile(request, connection.opts),
         :ok <- validate(connection, plan) do
      outputs =
        plan["nodes"]
        |> Enum.with_index()
        |> Enum.reduce(%{}, fn {node, id}, outputs ->
          rows =
            case node["kind"] do
              "source_sql" ->
                Stream.flat_map([:open], fn _ ->
                  connection.sources[node["source"]].query.(node["sql"])
                end)

              "input" ->
                if node["one_row"], do: [[]], else: []

              "local_project" ->
                Stream.map(outputs[node["input"]], fn row ->
                  Enum.map(node["expressions"], &evaluate(connection, &1, row))
                end)

              "source_request" ->
                input = if is_nil(node["input"]), do: [[]], else: outputs[node["input"]]

                input
                |> Stream.chunk_every(64)
                |> Stream.flat_map(fn rows ->
                  requests = Enum.map(rows, &bind(node["template"], &1))
                  connection.sources[node["source"]].execute_requests.(requests, node["columns"])
                end)

              "duckdb" ->
                inputs = Map.new(node["inputs"], &{&1["name"], outputs[&1["node"]]})
                Stream.flat_map([:open], fn _ -> connection.completion.(node["sql"], inputs) end)
            end

          rows =
            if reused?(connection, node, Enum.drop(plan["nodes"], id + 1)),
              do: spool(rows),
              else: rows

          Map.put(outputs, id, rows)
        end)

      {:ok, outputs[plan["result"]]}
    else
      {:ok, _} -> {:error, "Unsupported source-plan protocol"}
      error -> error
    end
  end

  defp validate(connection, plan) do
    nodes = plan["nodes"]

    if plan["version"] != 1 or not is_integer(plan["result"]) or plan["result"] < 0 or
         plan["result"] >= length(nodes),
       do: raise(ArgumentError, "Invalid source plan")

    Enum.with_index(nodes)
    |> Enum.each(fn {node, i} ->
      deps =
        case node["kind"] do
          "source_sql" ->
            source = Map.get(connection.sources, node["source"])

            if is_nil(source) or source.dialect != node["dialect"] or
                 not is_function(source[:query], 1),
               do: raise(ArgumentError, "Missing SQL source")

            []

          "source_request" ->
            source = Map.get(connection.sources, node["source"])

            if is_nil(source) or source.dialect != node["template"]["adapter"] or
                 not is_function(source[:execute_requests], 2),
               do: raise(ArgumentError, "Missing request source")

            if is_nil(node["input"]), do: [], else: [node["input"]]

          "local_project" ->
            input = node["input"]

            if not is_integer(input) or input < 0 or input >= i,
              do: raise(ArgumentError, "Invalid projection input")

            Enum.each(
              node["expressions"],
              &check_expression(connection, &1, length(Enum.at(nodes, input)["columns"]))
            )

            [input]

          "duckdb" ->
            if connection.engine != :duckdb or not is_function(connection.completion, 2),
              do: raise(ArgumentError, "DuckDB completion callback required")

            Enum.map(node["inputs"], & &1["node"])

          "input" ->
            []

          _ ->
            raise ArgumentError, "Unsupported plan operation"
        end

      if Enum.any?(deps, &(not is_integer(&1) or &1 < 0 or &1 >= i)),
        do: raise(ArgumentError, "Invalid plan dependency")
    end)

    :ok
  rescue
    error in ArgumentError -> {:error, Exception.message(error)}
  end

  defp check_expression(c, e, width) do
    case e["kind"] do
      "column" ->
        if e["index"] < 0 or e["index"] >= width, do: raise(ArgumentError, "Column out of bounds")

      "literal" ->
        if not Map.has_key?(e, "json"),
          do: raise(ArgumentError, "Literal needs an Arrow-capable client")

      kind when kind in ["cast", "graph_element"] ->
        check_expression(c, e["input"], width)

      "call" ->
        if not is_function(c.functions[e["function"]], 1),
          do: raise(ArgumentError, "Missing local function #{e["function"]}")

        Enum.each(e["arguments"], &check_expression(c, &1, width))

      _ ->
        raise ArgumentError, "Unsupported local expression"
    end
  end

  defp evaluate(c, e, row) do
    case e["kind"] do
      "column" ->
        Enum.fetch!(row, e["index"])

      "literal" ->
        e["json"]

      "cast" ->
        evaluate(c, e["input"], row)

      "graph_element" ->
        command = %{op: "decode_graph_values", data_type: e["data_type"], values: [evaluate(c, e["input"], row)]}
        case Runtime.compile(command, c.opts) do
          {:ok, [value]} -> value
          other -> raise ArgumentError, "Graph descriptor decoding failed: #{inspect(other)}"
        end

      "call" ->
        value = c.functions[e["function"]].(Enum.map(e["arguments"], &evaluate(c, &1, row)))
        validate_value(value, e["returns"])
        value
    end
  end

  defp validate_value(nil, _), do: :ok

  defp validate_value(value, type) do
    valid =
      case type do
        "int64" ->
          is_integer(value) and value >= -9_223_372_036_854_775_808 and
            value <= 9_223_372_036_854_775_807

        "int32" ->
          is_integer(value) and value >= -2_147_483_648 and value <= 2_147_483_647

        "float64" ->
          is_float(value)

        "float32" ->
          is_float(value)

        "boolean" ->
          is_boolean(value)

        "string" ->
          is_binary(value)

        _ ->
          true
      end

    if not valid, do: raise(ArgumentError, "Local function returned an invalid value for #{type}")

    if String.starts_with?(type, "list<") and String.ends_with?(type, ">") do
      if not is_list(value),
        do: raise(ArgumentError, "Local function must return a list for #{type}")

      item = String.slice(type, 5, String.length(type) - 6)
      Enum.each(value, &validate_value(&1, item))
    end

    :ok
  end

  defp bind(template, row) do
    if length(row) != template["parameters"],
      do: raise(ArgumentError, "Request parameter count mismatch")

    Enum.reduce(template["bindings"], template["request"], fn binding, request ->
      value = Enum.fetch!(row, binding["parameter"])
      value = if binding["encoding"] == "sql_null", do: is_nil(value), else: value

      parts =
        if binding["pointer"] == "",
          do: [],
          else:
            binding["pointer"]
            |> String.trim_leading("/")
            |> String.split("/")
            |> Enum.map(&(&1 |> String.replace("~1", "/") |> String.replace("~0", "~")))

      put_pointer(request, parts, value)
    end)
  end

  defp put_pointer(_, [], value), do: value

  defp put_pointer(object, [key | rest], value) when is_map(object),
    do: Map.put(object, key, put_pointer(Map.fetch!(object, key), rest, value))

  defp put_pointer(list, [key | rest], value) when is_list(list) do
    index = String.to_integer(key)
    List.replace_at(list, index, put_pointer(Enum.fetch!(list, index), rest, value))
  end

  defp reused?(c, node, later) do
    cond do
      node["kind"] == "duckdb" ->
        Enum.any?(later, &(&1["kind"] == "duckdb"))

      is_binary(node["source"]) ->
        source = c.sources[node["source"]]

        not Map.get(source, :concurrent, false) and
          Enum.any?(later, fn n ->
            n["source"] == node["source"] or
              (source[:concurrency_key] && c.sources[n["source"]] &&
                 c.sources[n["source"]][:concurrency_key] == source[:concurrency_key])
          end)

      true ->
        false
    end
  end

  defp spool(rows) do
    Stream.resource(
      fn ->
        path =
          Path.join(
            System.tmp_dir!(),
            "orchid-#{System.unique_integer([:positive, :monotonic])}.rows"
          )

        {:ok, file} = File.open(path, [:read, :write, :binary, :exclusive])

        try do
          Enum.each(rows, fn row ->
            bytes = :erlang.term_to_binary(row)
            :ok = IO.binwrite(file, <<byte_size(bytes)::unsigned-64, bytes::binary>>)
          end)

          {:ok, 0} = :file.position(file, 0)
          {file, path}
        rescue
          error ->
            File.close(file)
            File.rm(path)
            reraise error, __STACKTRACE__
        end
      end,
      fn {file, _} = state ->
        case IO.binread(file, 8) do
          :eof ->
            {:halt, state}

          <<size::unsigned-64>> ->
            {[:erlang.binary_to_term(IO.binread(file, size), [:safe])], state}

          other ->
            raise "Replay read failed: #{inspect(other)}"
        end
      end,
      fn {file, path} ->
        File.close(file)
        File.rm(path)
      end
    )
  end
end
