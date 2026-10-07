package io.orchiddb;

import java.util.*;

/** SQL can contain specialized parameter values. Treat it as potentially sensitive. */
public record CompiledQuery(
    String engine, SqlDialect dialect, String sql, List<String> fields, String diagnosticsJson) {
  public CompiledQuery(String engine, SqlDialect dialect, String sql, List<String> fields) {
    this(engine, dialect, sql, fields, "{}");
  }

  public CompiledQuery {
    Checks.name(engine);
    Objects.requireNonNull(dialect);
    Checks.name(sql);
    fields = List.copyOf(fields);
  }

  @Override
  public String toString() {
    return "CompiledQuery[engine="
        + engine
        + ", dialect="
        + dialect.id()
        + ", fields="
        + fields
        + "]";
  }
}
