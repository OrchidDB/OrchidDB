package io.orchiddb;

import java.sql.SQLException;
import java.util.List;
import org.apache.arrow.vector.VectorSchemaRoot;
import org.apache.arrow.vector.types.pojo.Field;
import org.apache.arrow.vector.util.Text;

/** Convenience view only: values are converted to Java objects when get() is called. */
final class ArrowRows implements QueryResult {
  private final ArrowResult result;
  private final List<String> columns;
  private VectorSchemaRoot batch;
  private int row = -1;
  private boolean closed;
  private boolean current;
  private boolean finished;

  ArrowRows(ArrowResult result) throws SQLException {
    this.result = result;
    try {
      for (var field : result.schema().getFields()) rejectDictionary(field);
      columns = result.schema().getFields().stream().map(f -> f.getName()).toList();
    } catch (SQLException | RuntimeException | Error e) {
      try {
        result.close();
      } catch (Throwable cleanup) {
        e.addSuppressed(cleanup);
      }
      throw e;
    }
  }

  private static void rejectDictionary(Field field) throws SQLException {
    if (field.getDictionary() != null)
      throw new java.sql.SQLFeatureNotSupportedException(
          "Use Arrow batches and dictionaries for dictionary-encoded columns");
    for (var child : field.getChildren()) rejectDictionary(child);
  }

  public List<String> columns() {
    return columns;
  }

  public boolean next() throws SQLException {
    if (closed) throw new SQLException("Result is closed");
    current = false;
    if (finished) return false;
    try {
      row++;
      while (batch == null || row >= batch.getRowCount()) {
        if (!result.nextBatch()) {
          finished = true;
          return false;
        }
        batch = result.batch();
        row = 0;
      }
      current = true;
      return true;
    } catch (SQLException | RuntimeException | Error e) {
      try {
        close();
      } catch (Throwable cleanup) {
        e.addSuppressed(cleanup);
      }
      throw e;
    }
  }

  public Object get(int column) throws SQLException {
    if (closed || !current) throw new SQLException("No current row");
    if (column < 1 || column > columns.size()) throw new SQLException("Invalid column: " + column);
    Object value = batch.getVector(column - 1).getObject(row);
    return value instanceof Text text ? text.toString() : value;
  }

  public void close() throws SQLException {
    if (closed) return;
    closed = true;
    current = false;
    result.close();
  }
}
