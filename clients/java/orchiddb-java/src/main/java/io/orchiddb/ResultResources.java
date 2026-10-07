package io.orchiddb;

import java.sql.SQLException;

final class ResultResources {
  private ResultResources() {}

  static void close(AutoCloseable... resources) throws SQLException {
    Throwable failure = null;
    for (var resource : resources) {
      if (resource == null) continue;
      try {
        resource.close();
      } catch (Throwable e) {
        if (failure == null) failure = e;
        else if (failure != e) failure.addSuppressed(e);
      }
    }
    if (failure instanceof SQLException sql) throw sql;
    if (failure instanceof RuntimeException runtime) throw runtime;
    if (failure instanceof Error error) throw error;
    if (failure != null) throw new SQLException("Could not close query resources", failure);
  }
}
