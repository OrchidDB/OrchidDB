package io.orchiddb;

import java.sql.ResultSet;
import java.sql.SQLException;
import org.apache.arrow.memory.BufferAllocator;
import org.apache.arrow.vector.ipc.ArrowReader;

/** Native driver bridge supplied by the application; must not consume rows through JDBC getters. */
@FunctionalInterface
public interface ArrowExporter {
  /**
   * Returned reader transfers to OrchidDB. On failure, clean up resources created by the exporter.
   */
  ArrowReader export(ResultSet result, BufferAllocator allocator, int batchSize)
      throws SQLException;
}
