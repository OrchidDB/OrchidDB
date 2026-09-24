import java.io.FileInputStream;
import java.io.FileOutputStream;
import org.apache.commons.configuration2.BaseConfiguration;
import org.apache.tinkerpop.gremlin.structure.io.graphml.GraphMLWriter;
import org.apache.tinkerpop.gremlin.structure.io.graphson.GraphSONMapper;
import org.apache.tinkerpop.gremlin.structure.io.graphson.GraphSONReader;
import org.apache.tinkerpop.gremlin.structure.io.graphson.GraphSONVersion;
import org.apache.tinkerpop.gremlin.structure.io.graphson.GraphSONXModuleV3;
import org.apache.tinkerpop.gremlin.structure.io.gryo.GryoWriter;
import org.apache.tinkerpop.gremlin.tinkergraph.structure.TinkerGraph;

/** Format conversion only. Input is an actual native snapshot, never a fixture or traversal result. */
public final class ExportGraph {
    public static void main(String[] args) throws Exception {
        if (args.length != 3) throw new IllegalArgumentException("ExportGraph writer native-graphson destination");
        var configuration = new BaseConfiguration();
        configuration.setProperty(TinkerGraph.GREMLIN_TINKERGRAPH_ALLOW_NULL_PROPERTY_VALUES, true);
        configuration.setProperty(TinkerGraph.GREMLIN_TINKERGRAPH_DEFAULT_VERTEX_PROPERTY_CARDINALITY, "list");
        try (var graph = TinkerGraph.open(configuration); var input = new FileInputStream(args[1])) {
            var mapper = GraphSONMapper.build().version(GraphSONVersion.V3_0).addCustomModule(GraphSONXModuleV3.build()).create();
            GraphSONReader.build().mapper(mapper).create().readGraph(input, graph);
            try (var output = new FileOutputStream(args[2])) {
                switch (args[0]) {
                    case "gryo" -> GryoWriter.build().create().writeGraph(output, graph);
                    case "graphml" -> GraphMLWriter.build().create().writeGraph(output, graph);
                    default -> throw new IllegalArgumentException("Unsupported writer: " + args[0]);
                }
            }
        }
    }
}
