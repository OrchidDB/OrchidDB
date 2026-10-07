defmodule OrchidDBRemoteEngineTest do
  use ExUnit.Case

  defp with_duckdb(run) do
    :ok = Adbc.download_driver(:duckdb)
    {:ok, database} = Adbc.Database.start_link(driver: :duckdb)
    {:ok, connection} = Adbc.Connection.start_link(database: database)

    try do
      run.(connection)
    after
      GenServer.stop(connection)
      GenServer.stop(database)
    end
  end

  defp sql_engine(connection) do
    %{
      dialect: "duckdb",
      query: fn sql, consume ->
        Adbc.Connection.query_pointer(connection, sql, consume)
      end
    }
  end

  defp consume(fields) do
    fn stream ->
      data =
        stream
        |> Adbc.StreamResult.to_ipc_stream()
        |> Adbc.Result.from_ipc_stream!()
        |> Adbc.Result.to_map()

      if map_size(data) == 0 do
        []
      else
        fields |> Enum.map(&Map.fetch!(data, &1)) |> Enum.zip() |> Enum.map(&Tuple.to_list/1)
      end
    end
  end

  test "dependent SQL uses typed inputs, concatenates results, and binds pending stages" do
    with_duckdb(fn connection ->
      column = %{"name" => "value", "data_type" => "int64", "nullable" => false}

      plan = %{
        "version" => 1,
        "dialect" => "duckdb",
        "execution_engine" => "local",
        "fields" => ["value"],
        "sql" => "SELECT value FROM stage2 ORDER BY value",
        "transfers" => [
          %{
            "source_engine" => "local",
            "source_dialect" => "duckdb",
            "sql" => "SELECT value AS seed FROM (VALUES (2::BIGINT),(5::BIGINT)) t(value)",
            "target_relation" => "stage1",
            "columns" => [column],
            "operation" => %{
              "engine" => "local",
              "input_columns" => [%{column | "name" => "seed"}],
              "template" => %{
                "dialect" => "duckdb",
                "parameters" => 1,
                "sql" => "SELECT CAST($1 + 1 AS BIGINT) AS value"
              }
            }
          },
          %{
            "source_engine" => "local",
            "source_dialect" => "duckdb",
            "sql" => "SELECT value * 2 AS value FROM stage1",
            "target_relation" => "stage2",
            "columns" => [column]
          }
        ]
      }

      engines = %{"local" => sql_engine(connection)}

      assert {:ok, [[6], [12]]} =
               OrchidDB.Internal.Runtime.execute_federated(engines, plan, consume(plan["fields"]))

      plan =
        put_in(plan, ["transfers", Access.at(0), "sql"], "SELECT 2::BIGINT AS seed WHERE FALSE")

      assert {:ok, []} = OrchidDB.Internal.Runtime.execute_federated(engines, plan, consume(plan["fields"]))
      assert {:ok, _} = Adbc.Connection.query(connection, "SELECT 1")
    end)
  end

  @tag skip: is_nil(System.get_env("ORCHIDDB_REMOTE_FIXTURE"))
  test "live Quickwit and Elasticsearch compose with caller-owned DuckDB sessions" do
    fixture = System.fetch_env!("ORCHIDDB_REMOTE_FIXTURE")

    cases =
      if File.regular?(fixture),
        do: fixture |> File.read!() |> Jason.decode!(),
        else: Jason.decode!(fixture)

    for test <- cases["cases"] do
      with_duckdb(fn connection ->
        for statement <- List.wrap(test["setup_sql"]),
            do: assert({:ok, _} = Adbc.Connection.query(connection, statement))

        {:ok, remote} =
          OrchidDB.RemoteEngine.start_link(test["adapter"], %{
            endpoint: test["endpoint"],
            page_size: 1,
            batch_size: 2
          })

        try do
          engines =
            Map.new(test["request"]["engines"], fn {name, engine} ->
              {name,
               if(engine["dialect"] == "duckdb",
                 do: sql_engine(connection),
                 else: OrchidDB.RemoteEngine.registry(remote)
               )}
            end)

          {:ok, plan} = OrchidDB.Internal.Runtime.compile(test["request"])
          callback = consume(plan["fields"])

          assert {:ok, test["expected_rows"]} ==
                   OrchidDB.Internal.Runtime.query_federated(engines, test["request"], callback)

          assert {:ok, _} = OrchidDB.RemoteEngine.clear_metadata_cache(remote)

          assert {:error, _} =
                   OrchidDB.RemoteEngine.execute_requests(remote, [%{invalid: true}], [])

          assert_raise RuntimeError, "consumer failed", fn ->
            OrchidDB.Internal.Runtime.query_federated(engines, test["request"], fn _ -> raise "consumer failed" end)
          end

          assert {:ok, test["expected_rows"]} ==
                   OrchidDB.Internal.Runtime.query_federated(engines, test["request"], callback)

          assert :ok = OrchidDB.RemoteEngine.close(remote)
          assert :ok = OrchidDB.RemoteEngine.close(remote)

          assert {:error, "Remote engine is closed"} =
                   OrchidDB.RemoteEngine.execute_requests(remote, [], [])

          assert {:ok, _} = Adbc.Connection.query(connection, "SELECT 1")
        after
          GenServer.stop(remote)
        end
      end)
    end
  end
end
