package io.crabgraph.gremlin.contract;

import io.crabgraph.gremlin.CrabGraph;
import org.apache.commons.configuration2.BaseConfiguration;
import org.apache.tinkerpop.gremlin.structure.*;
import org.apache.tinkerpop.gremlin.structure.util.GraphFactory;
import org.junit.*;
import java.lang.reflect.Method;
import java.nio.file.*;
import java.util.*;
import static org.junit.Assert.*;

/** Exercise the provider from another package, as upstream and external callers do. */
public class CrabProviderContractTest {
    private String executable;
    @Before public void nativeBinary() {
        executable=System.getenv("CRABGRAPH_JVM_STORE");
        Assume.assumeNotNull(executable);
    }
    @Test public void featureImplementationsExposePublicReflectiveMethods() throws Exception {
        try(CrabGraph graph=CrabGraph.open(executable)) {
            List<Object> pending=new ArrayList<>(); pending.add(graph.features());
            Set<Object> visited=Collections.newSetFromMap(new IdentityHashMap<>());
            int featureAccessors=0;
            for(int i=0;i<pending.size();i++) {
                Object feature=pending.get(i);
                if(!visited.add(feature)) continue;
                for(Method method:feature.getClass().getMethods()) {
                    if(method.getParameterCount()!=0) continue;
                    if(Graph.Features.FeatureSet.class.isAssignableFrom(method.getReturnType())) {
                        Object child=method.invoke(feature); assertNotNull(child); pending.add(child); featureAccessors++;
                    } else if(method.getName().startsWith("supports")&&method.getReturnType()==boolean.class) {
                        Object value=method.invoke(feature); assertNotNull(value);
                        if(feature instanceof Graph.Features.VariableFeatures) assertEquals(Boolean.FALSE,value);
                    }
                }
            }
            assertTrue(featureAccessors>=6);
            assertTrue(graph.toString().matches(".*\\[.*\\]"));
            assertTrue(graph.features().toString().startsWith("FEATURES"));
        }
    }
    @Test public void graphFactoryPreservesConfigurationAndCreatesMissingDirectories() throws Exception {
        Path directory=Files.createTempDirectory("crabgraph-factory-");
        BaseConfiguration configuration=new BaseConfiguration();
        configuration.setProperty(Graph.GRAPH,CrabGraph.class.getName());
        configuration.setProperty("crabgraph.native.executable",executable);
        configuration.setProperty("crabgraph.native.path",directory.resolve("missing/nested/graph.ngsp").toString());
        configuration.setProperty("application.setting","preserved");
        try {
            try(Graph graph=GraphFactory.open(configuration)) {
                assertSame(configuration,graph.configuration());
                assertEquals("preserved",graph.configuration().getString("application.setting"));
                graph.addVertex(T.id,"factory"); graph.tx().commit();
            }
            try(Graph graph=GraphFactory.open(configuration)) { assertTrue(graph.vertices("factory").hasNext()); }
        } finally {
            try(java.util.stream.Stream<Path> paths=Files.walk(directory)) {
                paths.sorted(Comparator.reverseOrder()).forEach(path->{try{Files.deleteIfExists(path);}catch(Exception ignored){}});
            }
        }
    }
    @Test public void unsupportedIdentifierTypesUseStandardElementExceptions() {
        try(CrabGraph graph=CrabGraph.open(executable)) {
            Vertex vertex=graph.addVertex();
            for(Object id:Arrays.asList(UUID.randomUUID(),new Date())) {
                UnsupportedOperationException vertexError=assertThrows(UnsupportedOperationException.class,()->graph.addVertex(T.id,id));
                assertEquals(Vertex.Exceptions.userSuppliedIdsOfThisTypeNotSupported().getMessage(),vertexError.getMessage());
                UnsupportedOperationException edgeError=assertThrows(UnsupportedOperationException.class,()->vertex.addEdge("self",vertex,T.id,id));
                assertEquals(Edge.Exceptions.userSuppliedIdsOfThisTypeNotSupported().getMessage(),edgeError.getMessage());
                UnsupportedOperationException propertyError=assertThrows(UnsupportedOperationException.class,
                    ()->vertex.property(VertexProperty.Cardinality.single,"x",1,T.id,id));
                assertEquals(VertexProperty.Exceptions.userSuppliedIdsOfThisTypeNotSupported().getMessage(),propertyError.getMessage());
            }
            assertEquals(Long.valueOf(1),graph.traversal().V().count().next());
            assertFalse(graph.edges().hasNext()); assertFalse(vertex.properties("x").hasNext());
        }
    }
}
