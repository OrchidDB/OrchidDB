package io.orchiddb;

import java.util.*;

/** Identifier parts are literal, never SQL. Use table(engine, schema, table) for qualification. */
public record Source(String engine, List<String> name) {
  public Source {
    Checks.name(engine);
    name = List.copyOf(name);
    if (name.isEmpty() || name.size() > 3)
      throw new IllegalArgumentException("Use one to three table identifier parts");
    name.forEach(Checks::name);
  }

  public static Source table(String engine, String... parts) {
    return new Source(engine, List.of(parts));
  }

  public String sql(SqlDialect dialect) {
    return String.join(".", name.stream().map(dialect::quote).toList());
  }
}
