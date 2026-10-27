defmodule OrchidDB.SourceConnectionTest do
  use ExUnit.Case
  alias OrchidDB.SourceConnection

  test "local function pipeline never invokes completion" do
    schema = %{
      functions: [
        %{name: "twice", target: "twice", parameters: ["int64"], returns: "int64", local: true}
      ]
    }

    for engine <- [:none, :duckdb] do
      assert {:ok, graph} =
               SourceConnection.new(schema, engine, %{},
                 functions: %{"twice" => fn [v] -> v * 2 end},
                 completion: fn _, _ -> flunk("DuckDB initialized") end
               )

      assert {:ok, rows} = SourceConnection.query(graph, "RETURN twice(21) AS answer")
      assert Enum.to_list(rows) == [[42]]
    end
  end

  test "source callback runs only when results are consumed" do
    parent = self()

    schema = %{
      sources: %{db: %{adapter: "trino"}},
      tables: [%{name: "people", source: "db", columns: [%{name: "id", data_type: "int64"}]}],
      nodes: [%{label: "Person", table: "people", id: "id", properties: %{id: "id"}}]
    }

    source = %{
      dialect: "trino",
      query: fn sql ->
        assert sql =~ "people"
        send(parent, :queried)
        [[1], [2]]
      end
    }

    assert {:ok, graph} = SourceConnection.new(schema, :none, %{"db" => source})
    assert {:ok, rows} = SourceConnection.query(graph, "MATCH (p:Person) RETURN p.id AS id")
    refute_received :queried
    assert Enum.to_list(rows) == [[1], [2]]
    assert_received :queried
  end
end
