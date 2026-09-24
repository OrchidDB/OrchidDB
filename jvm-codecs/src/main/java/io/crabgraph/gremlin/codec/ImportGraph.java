package io.crabgraph.gremlin.codec;

import java.io.FileInputStream;
import java.io.OutputStream;
import org.apache.commons.configuration2.BaseConfiguration;
import org.apache.tinkerpop.gremlin.structure.Direction;
import org.apache.tinkerpop.gremlin.structure.Graph;
import org.apache.tinkerpop.gremlin.structure.Element;
import org.apache.tinkerpop.gremlin.structure.io.graphml.GraphMLReader;
import org.apache.tinkerpop.gremlin.structure.io.graphson.GraphSONMapper;
import org.apache.tinkerpop.gremlin.structure.io.graphson.GraphSONReader;
import org.apache.tinkerpop.gremlin.structure.io.graphson.GraphSONVersion;
import org.apache.tinkerpop.gremlin.structure.io.graphson.GraphSONXModuleV3;
import org.apache.tinkerpop.gremlin.structure.io.gryo.GryoReader;
import org.apache.tinkerpop.gremlin.tinkergraph.structure.TinkerGraph;
import org.apache.tinkerpop.shaded.jackson.databind.ObjectMapper;
import org.apache.tinkerpop.shaded.jackson.core.JsonGenerator;
import org.apache.tinkerpop.gremlin.structure.Edge;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;


/** Real binary/XML codec only: the Rust write operator applies the decoded graph. */
public final class ImportGraph {
    public static void main(String[] args) throws Exception {
        if (args.length != 2) throw new IllegalArgumentException("ImportGraph reader path");
        var configuration = new BaseConfiguration();
        configuration.setProperty(TinkerGraph.GREMLIN_TINKERGRAPH_ALLOW_NULL_PROPERTY_VALUES, true);
        configuration.setProperty(TinkerGraph.GREMLIN_TINKERGRAPH_DEFAULT_VERTEX_PROPERTY_CARDINALITY, "list");
        var mapper = GraphSONMapper.build().version(GraphSONVersion.V3_0).addCustomModule(GraphSONXModuleV3.build()).create();
        try (var graph = TinkerGraph.open(configuration); var input = new FileInputStream(args[1])) {
            switch (args[0]) {
                case "graphson" -> GraphSONReader.build().mapper(mapper).create().readGraph(input, graph);
                case "gryo" -> GryoReader.build().create().readGraph(input, graph);
                case "graphml" -> GraphMLReader.build().create().readGraph(input, graph);
                default -> throw new IllegalArgumentException("Unsupported reader: " + args[0]);
            }
            writeNativeRecords(graph, mapper.createMapper(), System.out);
        }
    }

    // TinkerPop 3.7.4's StarGraphGraphSONSerializerV3 dereferences null edge and
    // meta-property values. Emit the standard adjacency envelope directly while
    // retaining the pinned GraphSON mapper for every typed value, including null.
    private static void field(JsonGenerator json, String name, Object value, ObjectMapper typed) throws java.io.IOException {
        json.writeFieldName(name);
        // Keep the mapper's exact numeric lexeme (decimal scale and signed zero)
        // instead of reparsing its typed output through a generic JSON number.
        json.writeRawValue(typed.writeValueAsString(value));
    }

    private static void properties(Element element, ObjectMapper typed, JsonGenerator json) throws java.io.IOException {
        json.writeObjectFieldStart("properties");
        var properties = element.properties();
        while (properties.hasNext()) {
            var property = properties.next();
            field(json, property.key(), property.value(), typed);
        }
        json.writeEndObject();
    }

    private static void writeNativeRecords(Graph graph, ObjectMapper typed, OutputStream output) throws Exception {
        var envelope = new ObjectMapper();
        try (var json = envelope.getFactory().createGenerator(output)) {
            json.disable(JsonGenerator.Feature.AUTO_CLOSE_TARGET);
            json.setRootValueSeparator(null);
            var vertices = graph.vertices();
            while (vertices.hasNext()) {
                var vertex = vertices.next();
                json.writeStartObject();
                field(json, "id", vertex.id(), typed);
                json.writeStringField("label", vertex.label());
                json.writeObjectFieldStart("properties");
                for (var key : vertex.keys()) {
                    json.writeArrayFieldStart(key);
                    var vertexProperties = vertex.properties(key);
                    while (vertexProperties.hasNext()) {
                        var property = vertexProperties.next();
                        json.writeStartObject();
                        field(json, "id", property.id(), typed);
                        field(json, "value", property.value(), typed);
                        properties(property, typed, json);
                        json.writeEndObject();
                    }
                    json.writeEndArray();
                }
                json.writeEndObject();
                for (var direction : new Direction[] {Direction.IN, Direction.OUT}) {
                    json.writeObjectFieldStart(direction == Direction.IN ? "inE" : "outE");
                    var edgeGroups = new LinkedHashMap<String,List<Edge>>();
                    vertex.edges(direction).forEachRemaining(edge -> edgeGroups.computeIfAbsent(edge.label(), k -> new ArrayList<>()).add(edge));
                    for (var entry : edgeGroups.entrySet()) {
                        json.writeArrayFieldStart(entry.getKey());
                        for (var edge : entry.getValue()) {
                            json.writeStartObject();
                            field(json, "id", edge.id(), typed);
                            field(json, direction == Direction.IN ? "outV" : "inV",
                                (direction == Direction.IN ? edge.outVertex() : edge.inVertex()).id(), typed);
                            properties(edge, typed, json);
                            json.writeEndObject();
                        }
                        json.writeEndArray();
                    }
                    json.writeEndObject();
                }
                json.writeEndObject();
                json.writeRaw('\n');
            }
        }
    }
}
