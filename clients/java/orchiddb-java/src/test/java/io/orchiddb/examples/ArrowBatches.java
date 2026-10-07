package io.orchiddb.examples;

import io.orchiddb.*;
import java.sql.DriverManager;
import java.util.List;
import java.util.Properties;
import org.apache.arrow.memory.RootAllocator;
import org.apache.arrow.vector.BigIntVector;
import org.apache.arrow.vector.ipc.ArrowReader;
import org.duckdb.DuckDBResultSet;

/** scripts/run-example.sh ArrowBatches */
public final class ArrowBatches {
  public static void main(String[] args) throws Exception {
    var compiler = NativeSqlCompiler.load();
    var settings = new Properties();
    // The caller explicitly chooses DuckDB execution streaming before opening its connection.
    settings.setProperty("jdbc_stream_results", "true");
    try (var connection = DriverManager.getConnection("jdbc:duckdb:", settings);
        var allocator = new RootAllocator(64 * 1024 * 1024)) {
      try (var statement = connection.createStatement()) {
        statement.execute("CREATE TABLE people AS SELECT i::BIGINT AS id FROM range(10000) t(i)");
      }
      var engine =
          JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection)
              .withArrow(
                  (rows, memory, batchSize) ->
                      (ArrowReader)
                          rows.unwrap(DuckDBResultSet.class).arrowExportStream(memory, batchSize),
                  allocator,
                  1024);
      var mapping =
          new GraphMapping(
              List.of(
                  NodeMapping.node("Person", Source.table("lake", "people"), "id")
                      .property("number", "id")),
              List.of());
      var graph = new OrchidDB(compiler, PlanCache.bounded(16), engine).graph(mapping);
      long count = 0, sum = 0;
      try (var result =
          graph.queryArrow(Query.cypher("MATCH (p:Person) RETURN p.number AS number"))) {
        System.out.println("Schema: " + result.schema());
        while (result.nextBatch()) {
          var batch = result.batch();
          var numbers = (BigIntVector) batch.getVector("number");
          for (int row = 0; row < batch.getRowCount(); row++) sum += numbers.get(row);
          count += batch.getRowCount();
          // Consume vectors here. The next batch reuses/replaces their buffers.
        }
      }
      if (count != 10000 || sum != 49995000) throw new AssertionError("Unexpected results");
      if (allocator.getAllocatedMemory() != 0 || !allocator.getChildAllocators().isEmpty())
        throw new AssertionError("Arrow resources leaked");
      System.out.println("Read " + count + " rows as Arrow batches; sum=" + sum);
      System.out.println("Caller connection remains open: " + !connection.isClosed());
    }
  }
}
