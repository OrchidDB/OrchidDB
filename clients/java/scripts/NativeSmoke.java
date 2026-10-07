import io.orchiddb.*;
import java.util.*;

/** Run against packaged artifacts, with no DuckDB driver or explicit native path. */
class NativeSmoke {
  public static void main(String[] args) {
    var compiler = NativeSqlCompiler.load();
    var source = Source.table("warehouse", "people");
    var mapping = new GraphMapping(
        List.of(NodeMapping.node("Person", source, "id").property("name", "name")), List.of());
    var request = new Compilation("warehouse", SqlDialect.DUCKDB, mapping,
        Map.of(source, List.of(new Column("id", "int64", false), new Column("name", "string", true))),
        List.of(), Query.cypher("MATCH (p:Person) RETURN p.name"));
    String sql = compiler.compile(request).sql();
    if (!sql.contains("people")) throw new AssertionError(sql);
    System.out.println("Packaged native compiler loaded and generated SQL without a database driver.");
  }
}
