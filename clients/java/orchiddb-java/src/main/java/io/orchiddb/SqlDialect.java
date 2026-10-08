package io.orchiddb;

/** Dialect identity is independent of transport and connection ownership. */
public record SqlDialect(String id) {
  public static final SqlDialect DUCKDB = new SqlDialect("duckdb");
  public static final SqlDialect POSTGRES = new SqlDialect("postgres");

  public static final SqlDialect STARROCKS = new SqlDialect("starrocks");

  public SqlDialect {
    Checks.name(id);
  }

  public String quote(String identifier) {
    String quote = id.equals("starrocks") ? "`" : "\"";
    return quote + Checks.name(identifier).replace(quote, quote + quote) + quote;
  }
}
