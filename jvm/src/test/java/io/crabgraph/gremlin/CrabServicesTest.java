package io.crabgraph.gremlin;

import static org.junit.Assert.*;
import java.util.*;
import org.junit.Test;
import org.junit.Assume;
import org.apache.tinkerpop.gremlin.structure.*;

public class CrabServicesTest {
    @Test public void servicesScanArbitraryNativeTopologyAndMetaProperties() {
        String binary=System.getenv("CRABGRAPH_JVM_STORE"); Assume.assumeNotNull(binary);
        try(CrabGraph graph=CrabGraph.open(binary)) {
            Vertex a=graph.addVertex(T.label,"custom","name","match-a");
            Vertex b=graph.addVertex(T.label,"other","name","b");
            a.property("tag","plain","note","match-meta");
            a.addEdge("connect",b,"description","match-edge");
            a.addEdge("self",a);
            var g=graph.traversal();
            assertEquals(3,g.call("tinker.search",Map.of("regex","match-.*")).toList().size());
            assertEquals(1,g.call("tinker.search",Map.of("search","match","type","VertexProperty")).toList().size());
            assertEquals(List.of(3L),g.V(a).call("tinker.degree.centrality",Map.of("direction",Direction.BOTH)).toList());
            assertEquals(Set.of("tinker.search","tinker.degree.centrality"),new HashSet<>(g.call().toList()));
            graph.tx().commit();
            b.addEdge("more",a);
            assertEquals(List.of(4L),g.V(a).call("tinker.degree.centrality",Map.of("direction",Direction.BOTH)).toList());
            graph.tx().rollback();
            assertEquals(List.of(3L),g.V(a).call("tinker.degree.centrality",Map.of("direction",Direction.BOTH)).toList());
        }
    }
}
