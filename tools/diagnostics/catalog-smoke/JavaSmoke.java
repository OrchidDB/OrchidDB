import io.orchiddb.*;
import java.nio.file.Path;
import java.sql.DriverManager;
import java.util.List;

class JavaSmoke {
  public static void main(String[] args) throws Exception {
    var e=System.getenv();
    var auths=List.of(CatalogAuth.bearer(e.get("AUTH_SMOKE_TOKEN")),
        CatalogAuth.bearer(Credential.environment("AUTH_SMOKE_TOKEN")),
        CatalogAuth.bearer(Credential.file(Path.of(e.get("AUTH_SMOKE_TOKEN_FILE")))),
        CatalogAuth.clientCredentials("admin",e.get("AUTH_SMOKE_SECRET")),
        CatalogAuth.clientCredentials("admin",Credential.environment("AUTH_SMOKE_SECRET")),
        CatalogAuth.clientCredentials("admin",Credential.file(Path.of(e.get("AUTH_SMOKE_SECRET_FILE")))),
        CatalogAuth.clientCredentials("external-client",e.get("AUTH_SMOKE_IDP_SECRET")).issuer(e.get("AUTH_SMOKE_ISSUER")),
        CatalogAuth.tokenExchange(Credential.value(e.get("AUTH_SMOKE_TOKEN"))));
    try(var db=DriverManager.getConnection("jdbc:duckdb:")) {
      db.createStatement().execute("CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,'Grace')");
      for(var auth:auths) {
        var catalog=new Catalog(e.get("AUTH_SMOKE_URL"),"smoke","smoke",auth);
        if(!catalog.discover().get("description").asText().equals("Auth smoke graph")) throw new AssertionError();
        var graph=new io.orchiddb.Connection(catalog,JdbcEngine.borrowed("local",SqlDialect.DUCKDB,db));
        try(var rows=graph.query("MATCH (p:Person)-[:PEER {score:1, limit:1}]->(q:Person) RETURN q.name AS name ORDER BY name")) {
          if(!rows.next() || !rows.get("name").equals("Ada") || !rows.next() || !rows.get("name").equals("Grace") || rows.next()) throw new AssertionError();
        }
      }
      var catalog=new Catalog(e.get("AUTH_SMOKE_URL"),"smoke","smoke",auths.get(3));
      if(catalog.registerPrincipal("java-workload",new Catalog.Principal("java-workload",List.of("reader"))).get("version").asInt()!=1) throw new AssertionError();
      if(catalog.principal("java-workload").get("version").asInt()!=1) throw new AssertionError();
    }
    System.out.println("PASS Java: credential sources, OAuth/OIDC, exchange, principal management, DuckDB execution");
  }
}
