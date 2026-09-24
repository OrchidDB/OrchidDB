package io.crabgraph.gremlin.computer;

import io.crabgraph.gremlin.CrabGraph;
import org.apache.commons.configuration2.Configuration;
import org.apache.tinkerpop.gremlin.process.computer.GraphComputer;
import org.apache.tinkerpop.gremlin.process.computer.GraphFilter;
import org.apache.tinkerpop.gremlin.process.computer.VertexComputeKey;
import org.apache.tinkerpop.gremlin.structure.*;
import org.apache.tinkerpop.gremlin.structure.util.ElementHelper;
import org.apache.tinkerpop.gremlin.structure.util.StringFactory;
import org.apache.tinkerpop.gremlin.util.iterator.IteratorUtils;

import java.util.*;
import java.util.function.Supplier;

/**
 * A computation-local view over the provider's actual graph. Only compute values
 * live in JVM memory; source elements and topology are always provider backed.
 * The executor serializes vertex execution and owns this view's lifetime.
 */
public final class ComputerView implements Graph {
    private final Graph source;
    private final boolean ownsSourceTransaction;
    private final Map<String, VertexComputeKey> computeKeys = new LinkedHashMap<>();
    private final Map<Object, Map<String, List<ComputeProperty<?>>>> computed = new LinkedHashMap<>();
    private final Set<Object> legalVertices = new HashSet<>();
    // The execution lease makes source records immutable for this view's lifetime.
    // Cache native-backed records, never a detached/reference graph implementation.
    private final Map<Object, Vertex> sourceVertices = new LinkedHashMap<>();
    private final Map<Object, List<VertexProperty<?>>> sourceVertexProperties = new HashMap<>();
    private final Map<Object, List<Property<?>>> sourceEdgeProperties = new HashMap<>();
    private final Map<Object, List<Property<?>>> sourceMetaProperties = new HashMap<>();
    private final Map<Object, Set<Object>> legalEdges = new HashMap<>();
    private final Map<Object, Set<Object>> legalProperties = new HashMap<>();
    private final GraphFilter filter;
    private boolean publishedOriginal;

    public ComputerView(Graph source, GraphFilter filter, Set<VertexComputeKey> keys) {
        this.source = Objects.requireNonNull(source);
        this.ownsSourceTransaction = source.features().graph().supportsTransactions() && !source.tx().isOpen();
        this.filter = Objects.requireNonNull(filter).clone();
        keys.forEach(key -> computeKeys.put(key.getKey(), key));
        try {
            source.vertices().forEachRemaining(vertex -> {
                if (!this.filter.legalVertex(vertex)) return;
                legalVertices.add(vertex.id());
                sourceVertices.put(vertex.id(), vertex);
                if (this.filter.hasEdgeFilter()) {
                    Set<Object> ids = new HashSet<>();
                    this.filter.legalEdges(vertex).forEachRemaining(edge -> ids.add(edge.id()));
                    legalEdges.put(vertex.id(), ids);
                }
                if (this.filter.hasVertexPropertyFilter()) {
                    Set<Object> ids = new HashSet<>();
                    this.filter.legalVertexProperties(vertex).forEachRemaining(property -> {
                        if (property instanceof VertexProperty) ids.add(((VertexProperty<?>) property).id());
                    });
                    legalProperties.put(vertex.id(), ids);
                }
            });
        } catch (RuntimeException | Error failure) {
            try { close(); } catch (RuntimeException close) { failure.addSuppressed(close); }
            throw failure;
        }
    }

    public Graph source() { return source; }
    public boolean legalVertex(Vertex vertex) { return legalVertices.contains(vertex.id()); }
    public boolean legalEdge(Vertex vertex, Edge edge) {
        return !filter.hasEdgeFilter() || legalEdges.getOrDefault(vertex.id(), Collections.emptySet()).contains(edge.id());
    }

    /** Drop transient execution objects before the map/reduce and publication phases. */
    public void complete() {
        computed.values().forEach(properties -> properties.keySet().removeIf(key -> computeKeys.get(key).isTransient()));
    }

    @Override public Iterator<Vertex> vertices(Object... ids) {
        Iterator<Vertex> vertices = ids.length == 0 ? sourceVertices.values().iterator()
                : IteratorUtils.filter(source.vertices(ids), this::legalVertex);
        return IteratorUtils.map(vertices, ViewVertex::new);
    }
    @Override public Iterator<Edge> edges(Object... ids) {
        return IteratorUtils.map(IteratorUtils.filter(source.edges(ids), edge ->
                legalVertex(edge.outVertex()) && legalVertex(edge.inVertex()) && legalEdge(edge.outVertex(), edge)), ViewEdge::new);
    }
    @Override public Vertex addVertex(Object... values) { throw immutable(); }
    @Override public GraphComputer compute() { throw immutable(); }
    @Override public <C extends GraphComputer> C compute(Class<C> type) { throw immutable(); }
    @Override public Transaction tx() { throw immutable(); }
    @Override public Variables variables() { throw immutable(); }
    @Override public Configuration configuration() { return source.configuration(); }
    @Override public Features features() { return source.features(); }
    @Override public void close() {
        // Reads may lazily open a transaction in the native provider. Never end a
        // transaction that was already open when the caller submitted the job.
        boolean interrupted = Thread.interrupted();
        try {
            if (!publishedOriginal && ownsSourceTransaction && source.tx().isOpen()) source.tx().rollback();
        } finally {
            if (interrupted) Thread.currentThread().interrupt();
        }
    }

    private static UnsupportedOperationException immutable() {
        return new UnsupportedOperationException("Only declared vertex compute properties may be modified during graph computation");
    }
    private void checkKey(String key) {
        if (!computeKeys.containsKey(key)) throw GraphComputer.Exceptions.providedKeyIsNotAnElementComputeKey(key);
    }
    private boolean retained(Vertex vertex, VertexProperty<?> property) {
        return !filter.hasVertexPropertyFilter() || legalProperties.getOrDefault(vertex.id(), Collections.emptySet()).contains(property.id());
    }
    private static boolean requested(String key, String... keys) {
        return keys == null || keys.length == 0 || Arrays.asList(keys).contains(key);
    }

    private List<VertexProperty<?>> sourceProperties(Vertex vertex) {
        return sourceVertexProperties.computeIfAbsent(vertex.id(), ignored -> new ArrayList<VertexProperty<?>>(IteratorUtils.list(vertex.<Object>properties())));
    }
    private List<Property<?>> sourceProperties(Edge edge) {
        return sourceEdgeProperties.computeIfAbsent(edge.id(), ignored -> new ArrayList<Property<?>>(IteratorUtils.list(edge.<Object>properties())));
    }
    private List<Property<?>> sourceProperties(VertexProperty<?> property) {
        return sourceMetaProperties.computeIfAbsent(property.id(), ignored -> new ArrayList<Property<?>>(IteratorUtils.list(property.<Object>properties())));
    }

    /**
     * Publish only final compute properties. NEW graphs come from the native
     * provider factory, never from a reference graph implementation.
     */
    public Graph finish(GraphComputer.ResultGraph result, GraphComputer.Persist persist, Supplier<Graph> freshGraph) {
        complete();
        if (result == GraphComputer.ResultGraph.ORIGINAL) {
            if (persist != GraphComputer.Persist.NOTHING) publishOriginal();
            else close();
            return source;
        }
        Graph target = Objects.requireNonNull(freshGraph.get(), "fresh graph");
        if (target == source) throw new IllegalArgumentException("NEW result graph factory returned the original graph");
        boolean transactional = target.features().graph().supportsTransactions();
        try {
            if (transactional && !target.tx().isOpen()) target.tx().open();
            Runnable copyGraph = () -> {
            if (persist != GraphComputer.Persist.NOTHING) {
                Map<Object, Vertex> vertices = new HashMap<>();
                List<Vertex> sourceVertices = IteratorUtils.list(vertices());
                sourceVertices.forEach(vertex -> {
                    interrupted();
                    Vertex copy = target.addVertex(T.id, vertex.id(), T.label, vertex.label());
                    vertices.put(vertex.id(), copy);
                    vertex.properties().forEachRemaining(property -> {
                        if (!generatedPropertyId(property)) copyProperty(copy, property);
                    });
                });
                // Reserve every existing/explicit ID before the provider allocates
                // IDs for new compute properties on any vertex.
                sourceVertices.forEach(vertex -> vertex.properties().forEachRemaining(property -> {
                    interrupted();
                    if (generatedPropertyId(property)) copyProperty(vertices.get(vertex.id()), property);
                }));
                if (persist == GraphComputer.Persist.EDGES) edges().forEachRemaining(edge -> {
                    interrupted();
                    Edge copy = vertices.get(edge.outVertex().id()).addEdge(edge.label(), vertices.get(edge.inVertex().id()), T.id, edge.id());
                    edge.properties().forEachRemaining(property -> copy.property(property.key(), property.value()));
                });
            }
            };
            if (target instanceof CrabGraph) ((CrabGraph) target).atomicMutation(copyGraph);
            else copyGraph.run();
            interrupted();
            if (transactional) target.tx().commit();
            close();
            return target;
        } catch (RuntimeException | Error failure) {
            if (transactional) try { target.tx().rollback(); } catch (RuntimeException rollback) { failure.addSuppressed(rollback); }
            try { target.close(); } catch (Exception close) { failure.addSuppressed(close); }
            try { close(); } catch (RuntimeException close) { failure.addSuppressed(close); }
            throw failure;
        }
    }

    private void publishOriginal() {
        if (source instanceof CrabGraph) {
            try {
                ((CrabGraph) source).atomicMutation(this::writeOriginalProperties);
                if (ownsSourceTransaction) {
                    interrupted();
                    source.tx().commit();
                }
                publishedOriginal = true;
            } catch (RuntimeException | Error failure) {
                try { close(); } catch (RuntimeException close) { failure.addSuppressed(close); }
                throw failure;
            }
            return;
        }
        boolean transactional = source.features().graph().supportsTransactions();
        if (transactional && !ownsSourceTransaction)
            throw new IllegalStateException("Persisting compute properties in a caller transaction requires provider savepoint support");
        try {
            if (transactional && !source.tx().isOpen()) source.tx().open();
            writeOriginalProperties();
            interrupted();
            if (transactional) source.tx().commit();
        } catch (RuntimeException | Error failure) {
            if (transactional) try { source.tx().rollback(); } catch (RuntimeException rollback) { failure.addSuppressed(rollback); }
            throw failure;
        }
    }
    private void writeOriginalProperties() {
        for (Map.Entry<Object, Map<String, List<ComputeProperty<?>>>> vertex : computed.entrySet()) {
            interrupted();
            Vertex target = source.vertices(vertex.getKey()).next();
            for (Map.Entry<String, List<ComputeProperty<?>>> entry : vertex.getValue().entrySet()) {
                // Include empty lists: removing a compute property is a persisted mutation.
                IteratorUtils.list(target.properties(entry.getKey())).forEach(VertexProperty::remove);
                entry.getValue().forEach(property -> copyProperty(target, property));
            }
        }
        interrupted();
    }
    private static void interrupted() {
        if (Thread.currentThread().isInterrupted()) throw new java.util.concurrent.CancellationException("Graph computation interrupted");
    }
    private static boolean generatedPropertyId(VertexProperty<?> property) {
        return property instanceof ComputerView.ComputeProperty<?> && ((ComputerView.ComputeProperty<?>) property).generatedId;
    }
    private static void copyProperty(Vertex target, VertexProperty<?> property) {
        // Computation-local IDs are not provider IDs. Let the native graph allocate
        // generated property identity according to its configured ID manager.
        boolean generated = generatedPropertyId(property);
        VertexProperty<?> copy = generated
                ? target.property(VertexProperty.Cardinality.list, property.key(), property.value())
                : target.property(VertexProperty.Cardinality.list, property.key(), property.value(), T.id, property.id());
        property.properties().forEachRemaining(meta -> copy.property(meta.key(), meta.value()));
    }

    private abstract class ViewElement implements Element {
        final Element base;
        ViewElement(Element base) { this.base = base; }
        @Override public Object id() { return base.id(); }
        @Override public String label() { return base.label(); }
        @Override public Graph graph() { return ComputerView.this; }
        @Override public void remove() { throw immutable(); }
        @Override public <V> Property<V> property(String key, V value) { throw immutable(); }
        @Override public boolean equals(Object other) { return ElementHelper.areEqual(this, other); }
        @Override public int hashCode() { return ElementHelper.hashCode(this); }
    }

    private final class ViewVertex extends ViewElement implements Vertex {
        final Vertex vertex;
        ViewVertex(Vertex vertex) { super(vertex); this.vertex = vertex; }
        @Override public Edge addEdge(String label, Vertex inVertex, Object... values) { throw immutable(); }
        @Override public <V> VertexProperty<V> property(String key) {
            Iterator<VertexProperty<V>> properties = properties(key);
            if (!properties.hasNext()) return VertexProperty.empty();
            VertexProperty<V> property = properties.next();
            if (properties.hasNext()) throw Vertex.Exceptions.multiplePropertiesExistForProvidedKey(key);
            return property;
        }
        @Override public <V> VertexProperty<V> property(String key, V value) {
            return property(features().vertex().getCardinality(key), key, value);
        }
        @Override public <V> VertexProperty<V> property(String key, V value, Object... values) {
            return property(features().vertex().getCardinality(key), key, value, values);
        }
        @Override public <V> VertexProperty<V> property(VertexProperty.Cardinality cardinality, String key, V value, Object... values) {
            checkKey(key);
            ElementHelper.validateProperty(key, value);
            ElementHelper.legalPropertyKeyValueArray(values);
            Map<String, List<ComputeProperty<?>>> properties = computed.computeIfAbsent(id(), ignored -> new LinkedHashMap<>());
            List<ComputeProperty<?>> list = properties.computeIfAbsent(key, ignored -> {
                List<ComputeProperty<?>> existing = new ArrayList<>();
                if (cardinality != VertexProperty.Cardinality.single) sourceProperties(vertex).forEach(property -> {
                    if (property.key().equals(key) && retained(vertex, property)) existing.add(new ComputeProperty<>(this, property));
                });
                return existing;
            });
            if (cardinality == VertexProperty.Cardinality.single) list.clear();
            if (cardinality == VertexProperty.Cardinality.set) for (ComputeProperty<?> property : list) {
                if (Objects.equals(property.value(), value)) {
                    ElementHelper.attachProperties(property, values);
                    return (VertexProperty<V>) property;
                }
            }
            Object propertyId = ElementHelper.getIdValue(values).orElseGet(() -> UUID.randomUUID().toString());
            ComputeProperty<V> property = new ComputeProperty<>(this, propertyId, key, value, ElementHelper.getIdValue(values).isEmpty());
            ElementHelper.attachProperties(property, values);
            list.add(property);
            return property;
        }
        @Override public <V> Iterator<VertexProperty<V>> properties(String... keys) {
            List<VertexProperty<V>> result = new ArrayList<>();
            Map<String, List<ComputeProperty<?>>> properties = computed.getOrDefault(id(), Collections.emptyMap());
            boolean allComputed = keys != null && keys.length > 0 && Arrays.stream(keys).allMatch(properties::containsKey);
            if (!allComputed) sourceProperties(vertex).forEach(property -> {
                if (requested(property.key(), keys) && !properties.containsKey(property.key()) && retained(vertex, property))
                    result.add(new SourceProperty<>(this, (VertexProperty<V>) property));
            });
            properties.forEach((key, list) -> {
                if (requested(key, keys)) list.forEach(property -> result.add((VertexProperty<V>) property));
            });
            return result.iterator();
        }
        @Override public Iterator<Edge> edges(Direction direction, String... labels) {
            return IteratorUtils.map(IteratorUtils.filter(vertex.edges(direction, labels), edge -> legalEdge(vertex, edge)), ViewEdge::new);
        }
        @Override public Iterator<Vertex> vertices(Direction direction, String... labels) {
            return IteratorUtils.map(edges(direction, labels), edge -> direction == Direction.OUT ? edge.inVertex() :
                    direction == Direction.IN ? edge.outVertex() : ElementHelper.areEqual(this, edge.outVertex()) ? edge.inVertex() : edge.outVertex());
        }
        @Override public String toString() { return StringFactory.vertexString(this); }
    }

    private final class ViewEdge extends ViewElement implements Edge {
        final Edge edge;
        ViewEdge(Edge edge) { super(edge); this.edge = edge; }
        @Override public Iterator<Vertex> vertices(Direction direction) {
            return IteratorUtils.map(edge.vertices(direction), ViewVertex::new);
        }
        @Override public <V> Iterator<Property<V>> properties(String... keys) {
            return IteratorUtils.map(IteratorUtils.filter(sourceProperties(edge).iterator(), property -> requested(property.key(), keys)),
                    property -> new ReadProperty<>(this, (Property<V>) property));
        }
        @Override public String toString() { return StringFactory.edgeString(this); }
    }

    private final class SourceProperty<V> extends ViewElement implements VertexProperty<V> {
        final ViewVertex owner;
        final VertexProperty<V> property;
        SourceProperty(ViewVertex owner, VertexProperty<V> property) { super(property); this.owner = owner; this.property = property; }
        @Override public String key() { return property.key(); }
        @Override public V value() { return property.value(); }
        @Override public boolean isPresent() { return property.isPresent(); }
        @Override public Vertex element() { return owner; }
        @Override public <U> Iterator<Property<U>> properties(String... keys) {
            return IteratorUtils.map(IteratorUtils.filter(sourceProperties(property).iterator(), meta -> requested(meta.key(), keys)),
                    meta -> new ReadProperty<>(this, (Property<U>) meta));
        }
        @Override public void remove() {
            checkKey(key());
            Map<String, List<ComputeProperty<?>>> properties = computed.computeIfAbsent(owner.id(), ignored -> new LinkedHashMap<>());
            List<ComputeProperty<?>> list = properties.computeIfAbsent(key(), ignored -> {
                List<ComputeProperty<?>> copy = new ArrayList<>();
                sourceProperties(owner.vertex).forEach(p -> { if (p.key().equals(key()) && retained(owner.vertex, p)) copy.add(new ComputeProperty<>(owner, p)); });
                return copy;
            });
            list.removeIf(p -> Objects.equals(p.id(), id()));
        }
        @Override public String toString() { return StringFactory.propertyString(this); }
    }

    private final class ComputeProperty<V> implements VertexProperty<V> {
        final ViewVertex owner;
        final Object id;
        final boolean generatedId;
        final String key;
        final V value;
        final Map<String, Object> meta = new LinkedHashMap<>();
        ComputeProperty(ViewVertex owner, Object id, String key, V value, boolean generatedId) {
            this.owner = owner; this.id = id; this.key = key; this.value = value; this.generatedId = generatedId;
        }
        ComputeProperty(ViewVertex owner, VertexProperty<V> property) {
            this(owner, property.id(), property.key(), property.value(), false);
            property.properties().forEachRemaining(p -> meta.put(p.key(), p.value()));
        }
        @Override public Object id() { return id; }
        @Override public String key() { return key; }
        @Override public V value() { return value; }
        @Override public boolean isPresent() { return true; }
        @Override public Vertex element() { return owner; }
        @Override public Graph graph() { return ComputerView.this; }
        @Override public void remove() {
            computed.getOrDefault(owner.id(), Collections.emptyMap()).getOrDefault(key, Collections.emptyList()).remove(this);
        }
        @Override public <U> Property<U> property(String key, U value) {
            ElementHelper.validateProperty(key, value);
            meta.put(key, value);
            return new MetaProperty<>(this, key);
        }
        @Override public <U> Iterator<Property<U>> properties(String... keys) {
            List<Property<U>> properties = new ArrayList<>();
            for (String key : meta.keySet()) if (requested(key, keys)) properties.add(new MetaProperty<>(this, key));
            return properties.iterator();
        }
        @Override public boolean equals(Object other) { return ElementHelper.areEqual((VertexProperty<?>) this, other); }
        @Override public int hashCode() { return ElementHelper.hashCode((Element) this); }
        @Override public String toString() { return StringFactory.propertyString(this); }
    }

    private static final class ReadProperty<V> implements Property<V> {
        final Element owner;
        final Property<V> property;
        ReadProperty(Element owner, Property<V> property) { this.owner = owner; this.property = property; }
        @Override public String key() { return property.key(); }
        @Override public V value() { return property.value(); }
        @Override public boolean isPresent() { return property.isPresent(); }
        @Override public Element element() { return owner; }
        @Override public void remove() { throw immutable(); }
        @Override public boolean equals(Object other) { return ElementHelper.areEqual(this, other); }
        @Override public int hashCode() { return ElementHelper.hashCode(this); }
        @Override public String toString() { return StringFactory.propertyString(this); }
    }
    private final class MetaProperty<V> implements Property<V> {
        final ComputeProperty<?> owner;
        final String key;
        MetaProperty(ComputeProperty<?> owner, String key) { this.owner = owner; this.key = key; }
        @Override public String key() { return key; }
        @Override public V value() {
            if (!isPresent()) throw Property.Exceptions.propertyDoesNotExist();
            return (V) owner.meta.get(key);
        }
        @Override public boolean isPresent() { return owner.meta.containsKey(key); }
        @Override public Element element() { return owner; }
        @Override public void remove() { owner.meta.remove(key); }
        @Override public boolean equals(Object other) { return ElementHelper.areEqual(this, other); }
        @Override public int hashCode() { return ElementHelper.hashCode(this); }
        @Override public String toString() { return StringFactory.propertyString(this); }
    }
}
