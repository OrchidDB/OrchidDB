package io.orchiddb;

/** Dialect identity is independent of transport and connection ownership. */
public record SqlDialect(String id) {
  public static final SqlDialect DUCKDB = new SqlDialect("duckdb");
  public static final SqlDialect POSTGRES = new SqlDialect("postgres");

  public SqlDialect {
    Checks.name(id);
  }

  public String quote(String identifier) {
    return "\"" + Checks.name(identifier).replace("\"", "\"\"") + "\"";
  }
}
