package io.orchiddb.examples;

import io.orchiddb.*;
import java.util.*;

/** No DuckDB/JDBC connection is created. Only schema metadata enters the compiler. */
public final class OfflineSql {
  public static void main(String[] args) {
    var compiler = NativeSqlCompiler.load();
    var source = Source.table("warehouse", "public", "people");
    var mapping =
        new GraphMapping(
            List.of(NodeMapping.node("Person", source, "id").property("name", "name")), List.of());
    var schemas =
        Map.of(
            source, List.of(new Column("id", "int64", false), new Column("name", "string", true)));
    for (var dialect : List.of(SqlDialect.DUCKDB, SqlDialect.POSTGRES)) {
      var request =
          new Compilation(
              "warehouse",
              dialect,
              mapping,
              schemas,
              List.of(),
              Query.cypher(
                  "MATCH (p:Person) WHERE p.name=$name RETURN p.name", Map.of("name", "Ada")));
      System.out.println(dialect.id() + ": " + compiler.compile(request).sql());
    }
  }
}
