defmodule OrchidDBTest do
  use ExUnit.Case

  def request(query) do
    %{
      version: 1,
      dialect: "duckdb",
      language: "cypher",
      query: query,
      tables: [
        %{
          name: "people",
          columns: [%{name: "id", data_type: "int64"}, %{name: "name", data_type: "string"}]
        }
      ],
      nodes: [
        %{label: "Person", table: "people", id: "id", properties: %{id: "id", name: "name"}}
      ]
    }
  end

  test "native compile, parameters and compiler errors" do
    assert {:ok, plan} = OrchidDB.Internal.Runtime.compile(request("MATCH (p:Person) RETURN p.name AS name"))
    assert plan["fields"] == ["name"]
    assert plan["sql"] =~ "people"

    assert {:ok, plan} =
             OrchidDB.Internal.Runtime.compile(Map.put(request("RETURN $n AS n"), :parameters, %{n: 42}))

    assert plan["sql"] =~ "42"
    assert {:error, _} = OrchidDB.Internal.Runtime.compile(request("MATCH (p:Person) DELETE p"))
  end

  test "provider-neutral permission scopes filter by direct and coarse resource keys" do
    grants = OrchidDB.Permission.relation("effective_grants", "document", "view")
    projects = OrchidDB.Permission.relation("effective_grants", "project", "view")

    request =
      Map.merge(request("MATCH (d:Document) RETURN d.title AS title"), %{
        authorization: OrchidDB.Permission.authorization("user", "alice"),
        tables: [
          %{
            name: "documents",
            columns: [
              %{name: "id", data_type: "int64"},
              %{name: "project_id", data_type: "int64"},
              %{name: "title", data_type: "string"}
            ]
          },
          %{
            name: "effective_grants",
            columns:
              Enum.map(
                [
                  "resource_type",
                  "resource_rel",
                  "resource_id",
                  "subject_type",
                  "subject_rel",
                  "subject_id"
                ],
                &%{name: &1, data_type: "string"}
              )
          }
        ],
        nodes: [
          %{
            label: "Document",
            table: "documents",
            id: "id",
            properties: %{title: "title", project_id: "project_id"},
            permission_scopes: [
              OrchidDB.Permission.scope("id", grants),
              OrchidDB.Permission.scope("project_id", projects)
            ]
          }
        ]
      })

    assert {:ok, plan} = OrchidDB.Internal.Runtime.compile(request)
    assert plan["sql"] =~ "effective_grants"
    assert plan["sql"] =~ "project_id"
    assert {:error, message} = OrchidDB.Internal.Runtime.compile(Map.delete(request, :authorization))
    assert message =~ "requires a principal"
  end

  test "shared statistics generation, reuse, persistence and clearing" do
    request = request("MATCH (p:Person) RETURN p.name AS name")

    collector = fn work ->
      assert work["max_rows"] > 0
      assert work["max_bytes"] > 0
      assert work["timeout_ms"] > 0
      {:error, "Test session cannot enforce bounded execution"}
    end

    assert {:ok, statistics} = OrchidDB.Internal.Statistics.generate(request, collector)
    assert is_map(statistics.snapshot)
    assert {:ok, plan} = OrchidDB.Internal.Runtime.compile(request, statistics: statistics)
    assert Map.has_key?(plan, "statistics_usage")
    assert Map.has_key?(plan, "plan_estimates")

    path =
      Path.join(
        System.tmp_dir!(),
        "orchiddb-statistics-#{System.unique_integer([:positive])}.json"
      )

    try do
      assert :ok = OrchidDB.Internal.Statistics.save(statistics, path)
      assert {:ok, loaded} = OrchidDB.Internal.Statistics.load(path)
      assert {:ok, recompiled} = OrchidDB.Internal.Statistics.compile(loaded, request)
      assert recompiled["sql"] == plan["sql"]
      assert {:ok, _} = OrchidDB.Internal.Statistics.clear(loaded)
    after
      File.rm(path)
      OrchidDB.Internal.Statistics.clear(statistics)
    end
  end

  test "bad library returns explicit error" do
    assert {:error, _} = OrchidDB.Internal.Runtime.compile(request("RETURN 1"), library: "/missing/compiler.so")
  end

  test "caller-owned DuckDB ADBC Arrow stream can be ingested without row conversion" do
    :ok = Adbc.download_driver(:duckdb)
    {:ok, db} = Adbc.Database.start_link(driver: :duckdb)
    {:ok, source} = Adbc.Connection.start_link(database: db)
    {:ok, sink} = Adbc.Connection.start_link(database: db)

    try do
      {:ok, _} = Adbc.Connection.query(source, "CREATE TABLE people(id BIGINT, name VARCHAR)")

      {:ok, _} =
        Adbc.Connection.query(
          source,
          "INSERT INTO people VALUES (9007199254740993, 'Ada'), (2, NULL)"
        )

      assert {:ok, _} =
               OrchidDB.Internal.Runtime.query_arrow(
                 source,
                 request("MATCH (p:Person) RETURN p.id AS id, p.name AS name ORDER BY id"),
                 fn stream ->
                   Adbc.Connection.bulk_insert!(sink, stream, table: "copied")
                 end
               )

      assert {:ok, result} =
               Adbc.Connection.query(sink, "SELECT id, name FROM copied ORDER BY id")

      assert Adbc.Result.to_map(result) == %{
               "id" => [2, 9_007_199_254_740_993],
               "name" => [nil, "Ada"]
             }

      assert_raise RuntimeError, "consumer failed", fn ->
        OrchidDB.Internal.Runtime.query_arrow(source, request("RETURN 1 AS answer"), fn _stream ->
          raise "consumer failed"
        end)
      end

      assert {:ok, _} = Adbc.Connection.query(source, "BEGIN")

      assert {:ok, _} =
               Adbc.Connection.query(source, "INSERT INTO people VALUES (3, 'transaction')")

      assert {:ok, _} =
               OrchidDB.Internal.Runtime.query_arrow(
                 source,
                 request("MATCH (p:Person) RETURN p.id AS id"),
                 fn stream ->
                   Adbc.Connection.bulk_insert!(sink, stream, table: "transaction_snapshot")
                 end
               )

      assert {:ok, result} =
               Adbc.Connection.query(sink, "SELECT count(*) AS n FROM transaction_snapshot")

      assert Adbc.Result.to_map(result) == %{"n" => [3]}
      assert {:ok, _} = Adbc.Connection.query(source, "ROLLBACK")
      assert {:ok, result} = Adbc.Connection.query(source, "SELECT count(*) AS n FROM people")
      assert Adbc.Result.to_map(result) == %{"n" => [2]}
      assert {:ok, result} = Adbc.Connection.query(source, "SELECT version() AS version")
      assert %{"version" => ["v" <> _]} = Adbc.Result.to_map(result)
    after
      GenServer.stop(source)
      GenServer.stop(sink)
      GenServer.stop(db)
    end
  end
end
