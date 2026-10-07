package io.orchiddb;

/** Internal work limits supplied by the shared coordinator, not user tuning settings. */
public record StatisticsRead(String sql, long maxRows, long maxBytes, long timeoutMillis) {
  public StatisticsRead {
    Checks.name(sql);
    if (maxRows <= 0 || maxBytes <= 0 || timeoutMillis <= 0)
      throw new IllegalArgumentException("Statistics work limits must be positive");
  }
}
