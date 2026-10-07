defmodule OrchidDBFederationTest do
  use ExUnit.Case

  test "SQL islands route to the source and only queries are issued despite consumer failure" do
    request = %{
      version: 1,
      dialect: "duckdb",
      execution_engine: "d",
      engines: %{d: %{dialect: "duckdb"}, p: %{dialect: "postgres"}},
      language: "cypher",
      query: "MATCH (p:Person) WHERE p.age > 25 RETURN count(p) AS n",
      tables: [
        %{
          name: "people",
          engine: "p",
          columns: [%{name: "id", data_type: "int64"}, %{name: "age", data_type: "int64"}]
        }
      ],
      nodes: [%{label: "Person", table: "people", id: "id", properties: %{age: "age"}}]
    }

    owner = self()

    engines = %{
      "p" => %{
        dialect: "postgres",
        query: fn sql, consume ->
          assert sql =~ "25"
          assert String.downcase(sql) =~ "count("
          send(owner, :source)
          consume.([[1]])
        end
      },
      "d" => %{
        dialect: "duckdb",
        query: fn sql, consume ->
          assert sql =~ "WITH"
          assert sql =~ "VALUES"
          refute sql =~ "CREATE"
          send(owner, :bound_query)
          consume.(:result_stream)
        end
      }
    }

    assert_raise RuntimeError, "consumer failed", fn ->
      OrchidDB.query_federated(engines, request, fn :result_stream -> raise "consumer failed" end)
    end

    assert_received :source
    assert_received :bound_query
  end

  for {target, source} <- [{"duckdb", "postgres"}, {"postgres", "duckdb"}] do
    @tag skip: is_nil(System.get_env("ORCHIDDB_TEST_PG_URI"))
    test "ADBC streams cross engines without database objects into #{target}" do
      :ok = Adbc.download_driver(:duckdb)
      :ok = Adbc.download_driver(:postgresql)
      {:ok, ddb} = Adbc.Database.start_link(driver: :duckdb)

      {:ok, pdb} =
        Adbc.Database.start_link(
          driver: :postgresql,
          uri: System.fetch_env!("ORCHIDDB_TEST_PG_URI")
        )

      {:ok, d} = Adbc.Connection.start_link(database: ddb)
      {:ok, p} = Adbc.Connection.start_link(database: pdb)

      try do
        for conn <- [d, p] do
          {:ok, _} =
            Adbc.Connection.query(conn, "CREATE TEMP TABLE people(id BIGINT, name VARCHAR)")

          {:ok, _} =
            Adbc.Connection.query(
              conn,
              "INSERT INTO people VALUES (9007199254740993, 'Ada'), (2, NULL)"
            )
        end

        engines =
          Map.new([{"duckdb", d}, {"postgres", p}], fn {dialect, conn} ->
            {dialect,
             %{
               dialect: dialect,
               query: fn sql, callback ->
                 assert Regex.match?(~r/^\s*(SELECT|WITH)\b/i, sql)
                 Adbc.Connection.query_pointer(conn, sql, callback)
               end
             }}
          end)

        request =
          OrchidDBTest.request("MATCH (p:Person) RETURN p.id AS id, p.name AS name ORDER BY id")

        request =
          Map.merge(request, %{
            dialect: unquote(target),
            execution_engine: unquote(target),
            engines: %{duckdb: %{dialect: "duckdb"}, postgres: %{dialect: "postgres"}},
            tables: Enum.map(request.tables, &Map.put(&1, :engine, unquote(source)))
          })

        consume = fn stream ->
          stream
          |> Adbc.StreamResult.to_ipc_stream()
          |> Adbc.Result.from_ipc_stream!()
          |> Adbc.Result.to_map()
        end

        assert {:ok, %{"id" => [2, 9_007_199_254_740_993], "name" => [nil, "Ada"]}} =
                 OrchidDB.query_federated(engines, request, consume)

        assert_raise RuntimeError, "consumer failed", fn ->
          OrchidDB.query_federated(engines, request, fn _ -> raise "consumer failed" end)
        end

        {:ok, _} =
          Adbc.Connection.query(d, "CREATE TEMP TABLE nested_values(id BIGINT, items BIGINT[][])")

        {:ok, _} =
          Adbc.Connection.query(d, "INSERT INTO nested_values VALUES (1, [[1],[2,3],NULL,[]])")

        {:ok, _} =
          Adbc.Connection.query(p, "CREATE TEMP TABLE nested_values(id BIGINT, items JSONB[])")

        {:ok, _} =
          Adbc.Connection.query(
            p,
            "INSERT INTO nested_values VALUES (1, ARRAY['[1]'::jsonb,'[2,3]'::jsonb,NULL,'[]'::jsonb])"
          )

        nested = %{
          request
          | query: "MATCH (n:Nested) RETURN ncount(n.items) AS n",
            tables: [
              %{
                name: "nested_values",
                engine: unquote(source),
                columns: [
                  %{name: "id", data_type: "int64"},
                  %{name: "items", data_type: "list:list:int64"}
                ]
              }
            ],
            nodes: [
              %{label: "Nested", table: "nested_values", id: "id", properties: %{items: "items"}}
            ]
        }

        nested =
          Map.put(nested, :functions, [
            %{
              name: "ncount",
              target: if(unquote(target) == "postgres", do: "cardinality", else: "len"),
              parameters: ["list:list:int64"],
              returns: "int64"
            }
          ])

        assert {:ok, %{"n" => [4]}} = OrchidDB.query_federated(engines, nested, consume)
        for conn <- [d, p], do: assert({:ok, _} = Adbc.Connection.query(conn, "SELECT 1"))
      after
        GenServer.stop(d)
        GenServer.stop(p)
        GenServer.stop(ddb)
        GenServer.stop(pdb)
      end
    end
  end
end
