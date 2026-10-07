package io.orchiddb;

import java.util.*;

public record Query(
    String language,
    String text,
    Map<String, Object> parameters,
    Ontology ontology,
    List<Map<String, Object>> rdf,
    String dataset,
    Authorization authorization) {
  public Query(String language, String text, Map<String, Object> parameters, Ontology ontology) {
    this(language, text, parameters, ontology, List.of(), "default", null);
  }

  public Query(
      String language,
      String text,
      Map<String, Object> parameters,
      Ontology ontology,
      List<Map<String, Object>> rdf,
      String dataset) {
    this(language, text, parameters, ontology, rdf, dataset, null);
  }

  @SuppressWarnings("unchecked")
  public Query {
    Checks.name(language);
    Checks.name(text);
    Objects.requireNonNull(ontology);
    Checks.name(dataset);
    rdf = rdf.stream().map(rule -> (Map<String, Object>) freeze(rule)).toList();
    var copy = new TreeMap<String, Object>();
    parameters.forEach((k, v) -> copy.put(Checks.name(k), freeze(v)));
    parameters = Collections.unmodifiableMap(copy);
    if (!language.equals("cypher") && !parameters.isEmpty())
      throw new IllegalArgumentException("Bindings currently supported only for Cypher");
  }

  private static Object freeze(Object v) {
    if (v == null
        || v instanceof String
        || v instanceof Boolean
        || v instanceof Byte
        || v instanceof Short
        || v instanceof Integer
        || v instanceof Long) return v;
    if (v instanceof Double d && Double.isFinite(d)) return d;
    if (v instanceof Float f && Float.isFinite(f)) return f;
    if (v instanceof List<?> l)
      return Collections.unmodifiableList(l.stream().map(Query::freeze).toList());
    if (v instanceof Map<?, ?> m) {
      var r = new TreeMap<String, Object>();
      m.forEach(
          (k, x) -> {
            if (!(k instanceof String s))
              throw new IllegalArgumentException("Parameter map keys must be strings");
            r.put(s, freeze(x));
          });
      return Collections.unmodifiableMap(r);
    }
    throw new IllegalArgumentException("Unsupported parameter type: " + v.getClass().getName());
  }

  public static Query cypher(String text) {
    return cypher(text, Map.of());
  }

  public static Query cypher(String text, Map<String, Object> parameters) {
    return new Query("cypher", text, parameters, Ontology.EMPTY);
  }

  public Query as(Authorization principal) {
    return new Query(
        language, text, parameters, ontology, rdf, dataset, Objects.requireNonNull(principal));
  }

  public static Query gremlin(String text) {
    return new Query("gremlin", text, Map.of(), Ontology.EMPTY);
  }

  public static Query sparql(String text, List<Map<String, Object>> rdf, String dataset) {
    return new Query("sparql", text, Map.of(), Ontology.EMPTY, rdf, dataset);
  }

  public static Query sparql(String text, Ontology ontology) {
    return new Query("sparql", text, Map.of(), ontology);
  }
}
