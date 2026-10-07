package io.orchiddb;

import java.sql.SQLException;
import java.util.List;

/** Streaming cursor. Always close it, including after partial iteration. Columns are 1-based. */
public interface QueryResult extends AutoCloseable {
  List<String> columns();

  boolean next() throws SQLException;

  Object get(int column) throws SQLException;

  default Object get(String name) throws SQLException {
    int index = columns().indexOf(name);
    if (index < 0) throw new SQLException("Unknown result column: " + name);
    if (columns().lastIndexOf(name) != index)
      throw new SQLException("Ambiguous result column: " + name);
    return get(index + 1);
  }

  void close() throws SQLException;
}
