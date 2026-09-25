package io.orchiddb.gremlin;

import org.apache.tinkerpop.gremlin.structure.*;
import org.junit.*;
import java.util.*;
import static org.junit.Assert.*;

public class OrchidPropertyCacheTest {
    @Test public void cachedPropertiesPreserveCardinalityMutableValuesAndTransactionChanges() {
        String executable=System.getenv("ORCHIDDB_JVM_STORE");
        Assume.assumeTrue(executable!=null);
        String setting="orchiddb.native.propertyCacheSize",previous=System.getProperty(setting);
        try {
            for(int capacity:new int[]{0,2,16384}) {
                System.setProperty(setting,Integer.toString(capacity));
                try(OrchidGraph graph=OrchidGraph.open(executable)) {
                    Vertex v=graph.addVertex("x",1,"x",2);
                    Edge edge=v.addEdge("self",v,"values",Arrays.asList(1,2));
                    VertexProperty<Integer> property=v.property(VertexProperty.Cardinality.list,"x",3,"meta",7);
                    graph.tx().commit();
                    for(int repeat=0;repeat<2;repeat++) {
                        assertEquals(Arrays.asList(1,2,3),graph.traversal().V(v).values("x").toList());
                        edge.<List<Integer>>value("values").clear();
                        assertEquals(Arrays.asList(1,2),edge.value("values"));
                        assertEquals(Integer.valueOf(7),property.value("meta"));
                    }
                    edge.property("values",Arrays.asList(4)); property.property("meta",8);
                    assertEquals(Arrays.asList(4),edge.value("values"));
                    assertEquals(Integer.valueOf(8),property.value("meta"));
                    graph.tx().rollback();
                    assertEquals(Arrays.asList(1,2),edge.value("values"));
                    assertEquals(Integer.valueOf(7),property.value("meta"));
                    graph.atomicMutation(()->{
                        assertEquals(Arrays.asList(1,2),edge.value("values"));
                        edge.property("values",Arrays.asList(9));
                        assertEquals(Arrays.asList(9),edge.value("values"));
                    });
                    graph.tx().rollback();
                    assertEquals(Arrays.asList(1,2),edge.value("values"));
                }
            }
        } finally { if(previous==null) System.clearProperty(setting); else System.setProperty(setting,previous); }
    }
}
