alias OrchidDB.{Catalog, CatalogAuth, Credential}
e = &System.fetch_env!/1
{:ok, _} = Application.ensure_all_started(:adbc)
{:ok, database} = Adbc.Database.start_link(driver: :duckdb)
{:ok, session} = Adbc.Connection.start_link(database: database)
{:ok, _} = Adbc.Connection.query(session, "CREATE TABLE people(id BIGINT, name VARCHAR)")
{:ok, _} = Adbc.Connection.query(session, "INSERT INTO people VALUES (1,'Ada'),(2,'Grace')")
auths = [CatalogAuth.bearer(e.("AUTH_SMOKE_TOKEN")), CatalogAuth.bearer(Credential.env("AUTH_SMOKE_TOKEN")),
  CatalogAuth.bearer(Credential.file(e.("AUTH_SMOKE_TOKEN_FILE"))), CatalogAuth.client_credentials("admin",e.("AUTH_SMOKE_SECRET")),
  CatalogAuth.client_credentials("admin",Credential.env("AUTH_SMOKE_SECRET")), CatalogAuth.client_credentials("admin",Credential.file(e.("AUTH_SMOKE_SECRET_FILE"))),
  CatalogAuth.client_credentials("external-client",e.("AUTH_SMOKE_IDP_SECRET"),issuer: e.("AUTH_SMOKE_ISSUER")),
  CatalogAuth.token_exchange(e.("AUTH_SMOKE_TOKEN"))]
for auth <- auths do
  catalog = Catalog.new(e.("AUTH_SMOKE_URL"),scope: "smoke",graph: "smoke",auth: auth)
  {:ok, %{"description" => "Auth smoke graph"}} = Catalog.discover(catalog)
  {:ok, graph} = OrchidDB.connect(session,catalog)
  {:ok, result} = OrchidDB.query(graph,"MATCH (p:Person)-[:PEER {score:1, limit:1}]->(q:Person) RETURN q.name AS name ORDER BY name")
  %{"name" => ["Ada", "Grace"]} = Adbc.Result.to_map(result)
end
catalog = Catalog.new(e.("AUTH_SMOKE_URL"),scope: "smoke",graph: "smoke",auth: Enum.at(auths,3))
{:ok, %{"version" => 1}} = Catalog.register_principal(catalog,"elixir-workload",roles: ["reader"])
{:ok, %{"version" => 1}} = Catalog.principal(catalog,"elixir-workload")
GenServer.stop(session)
GenServer.stop(database)
IO.puts("PASS Elixir: credential sources, OAuth/OIDC, exchange, principal management, DuckDB execution")
