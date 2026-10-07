request = %{
  version: 1,
  dialect: "duckdb",
  language: "cypher",
  query: "RETURN $answer AS answer",
  parameters: %{answer: 42},
  tables: [],
  nodes: []
}

{:ok, plan} = OrchidDB.compile(request)
IO.puts(plan["sql"])
