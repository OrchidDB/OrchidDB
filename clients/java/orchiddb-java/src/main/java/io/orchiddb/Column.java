package io.orchiddb;

/** Compiler schema type, e.g. int64, string, decimal:18:2. See README for supported types. */
public record Column(String name, String dataType, boolean nullable) {
  public Column {
    Checks.name(name);
    Checks.name(dataType);
  }
}
