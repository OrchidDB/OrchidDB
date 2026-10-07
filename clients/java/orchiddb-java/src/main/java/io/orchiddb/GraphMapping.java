package io.orchiddb;

import java.util.*;

public record GraphMapping(
    List<NodeMapping> nodes,
    List<EdgeMapping> edges,
    Ontology ontology,
    List<Map<String, Object>> rdf,
    String dataset) {
  public GraphMapping(List<NodeMapping> nodes, List<EdgeMapping> edges) {
    this(nodes, edges, Ontology.EMPTY, List.of(), "default");
  }

  public GraphMapping {
    Objects.requireNonNull(ontology);
    rdf = rdf.stream().map(GraphMapping::freezeRdf).toList();
    Objects.requireNonNull(dataset);
    nodes = List.copyOf(nodes);
    edges = List.copyOf(edges);
    if (nodes.isEmpty()) throw new IllegalArgumentException("Map at least one node label");
    var labels = new HashSet<String>();
    for (var n : nodes)
      if (!labels.add(n.label()))
        throw new IllegalArgumentException("Duplicate node label " + n.label());
    var rels = new HashSet<String>();
    for (var e : edges) {
      if (!rels.add(e.label()))
        throw new IllegalArgumentException("Duplicate edge label " + e.label());
      if (!labels.contains(e.sourceLabel()) || !labels.contains(e.targetLabel()))
        throw new IllegalArgumentException("Unmapped edge endpoint label");
    }
  }

  @SuppressWarnings("unchecked")
  private static Map<String, Object> freezeRdf(Map<String, Object> mapping) {
    return (Map<String, Object>) Query.freeze(mapping);
  }

  public Set<Source> sources() {
    var sources = new LinkedHashSet<Source>();
    nodes.forEach(n -> sources.add(n.source()));
    edges.forEach(e -> sources.add(e.source()));
    nodes.stream()
        .flatMap(n -> n.permissionScopes().stream())
        .forEach(scope -> sources.add(scope.relation().source()));
    return Collections.unmodifiableSet(sources);
  }

  public String singleEngine() {
    var engines = new TreeSet<String>();
    sources().forEach(s -> engines.add(s.engine()));
    if (engines.size() != 1)
      throw new PlanningException(
          "Cross-engine queries require a federation coordinator, which is not implemented: "
              + engines);
    return engines.first();
  }
}
