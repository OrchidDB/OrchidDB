package io.orchiddb;

import java.sql.SQLException;
import java.util.*;

/** Execution transport for SQL islands and typed non-SQL request operations. */
public interface ExecutionEngine {
  String id();

  SqlDialect dialect();

  Session openSession() throws SQLException;

  interface Session extends AutoCloseable {
    Map<Source, List<Column>> schemas(Set<Source> sources) throws SQLException;

    /** Adapters may inspect only the columns needed by this graph mapping. */
    default Map<Source, List<Column>> schemas(GraphMapping mapping) throws SQLException {
      return schemas(mapping.sources());
    }

    /** Batch execution; adapters must implement a native batch path or reject explicitly. */
    default ArrowResult executeArrow(CompiledQuery query) throws SQLException {
      throw new java.sql.SQLFeatureNotSupportedException("This engine has no Arrow result adapter");
    }

    /** Row convenience for Arrow adapters. Legacy row-only adapters may override this. */
    default QueryResult execute(CompiledQuery query) throws SQLException {
      return executeArrow(query).rows();
    }

    /**
     * Execute bound non-SQL requests against a declared exchange schema.
     *
     * @param requestsJson JSON array of bound request payloads
     * @param columnsJson JSON array of shared-protocol transfer columns
     * @return JSON array of rows in the shared bind format (JSON-domain cells are JSON text)
     */
    default String executeRequests(String requestsJson, String columnsJson) throws SQLException {
      throw new java.sql.SQLFeatureNotSupportedException("This engine has no request adapter");
    }

    /** Execute a bounded collection request; unsupported adapters report missing coverage. */
    default QueryResult readStatistics(StatisticsRead request) throws SQLException {
      throw new java.sql.SQLFeatureNotSupportedException(
          "This engine has no bounded statistics reader");
    }

    void close() throws SQLException;
  }
}
