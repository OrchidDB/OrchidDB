package io.orchiddb;

import java.sql.SQLException;
import org.apache.arrow.vector.VectorSchemaRoot;
import org.apache.arrow.vector.dictionary.DictionaryProvider;
import org.apache.arrow.vector.types.pojo.Schema;

/**
 * Batch cursor. Schema is available before reading, including for empty results. A batch and its
 * vectors are borrowed until nextBatch() or close(); do not close or mutate them. Copy/retain data
 * explicitly if it must outlive that boundary. The result owns its reader and query allocator, not
 * the caller's parent allocator. Close even after exhausting the stream. Not thread-safe.
 */
public interface ArrowResult extends AutoCloseable {
  Schema schema() throws SQLException;

  /** Borrowed dictionaries, valid only while this result is open. */
  DictionaryProvider dictionaries() throws SQLException;

  boolean nextBatch() throws SQLException;

  VectorSchemaRoot batch() throws SQLException;

  void close() throws SQLException;

  /** Transfer iteration/close ownership to a row view; do not mix batch and row iteration. */
  default QueryResult rows() throws SQLException {
    return new ArrowRows(this);
  }
}
