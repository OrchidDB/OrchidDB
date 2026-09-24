import java.io.FileInputStream;
import org.apache.tinkerpop.gremlin.structure.io.graphml.GraphMLReader;
import org.apache.tinkerpop.gremlin.structure.io.graphson.GraphSONMapper;
import org.apache.tinkerpop.gremlin.structure.io.graphson.GraphSONVersion;
import org.apache.tinkerpop.gremlin.structure.io.graphson.GraphSONWriter;
import org.apache.tinkerpop.gremlin.structure.io.gryo.GryoReader;
import org.apache.tinkerpop.gremlin.tinkergraph.structure.TinkerGraph;

/** Real binary/XML codec only: the Rust write operator applies the decoded graph. */
public final class ImportGraph {
    public static void main(String[] args) throws Exception {
        if (args.length != 2) throw new IllegalArgumentException("ImportGraph reader path");
        try (var graph = TinkerGraph.open(); var input = new FileInputStream(args[1])) {
            switch (args[0]) {
                case "gryo" -> GryoReader.build().create().readGraph(input, graph);
                case "graphml" -> GraphMLReader.build().create().readGraph(input, graph);
                default -> throw new IllegalArgumentException("Unsupported reader: " + args[0]);
            }
            var mapper = GraphSONMapper.build().version(GraphSONVersion.V3_0).create();
            GraphSONWriter.build().mapper(mapper).create().writeGraph(System.out, graph);
        }
    }
}
