package io.orchiddb;

import java.util.*;

/** Each edge needs a stable unique integer ID, including parallel edges. */
public record EdgeMapping(
    String label,
    Source source,
    String id,
    String sourceColumn,
    String targetColumn,
    String sourceLabel,
    String targetLabel,
    Map<String, String> properties) {
  public EdgeMapping {
    Checks.name(label);
    Objects.requireNonNull(source);
    Checks.name(id);
    Checks.name(sourceColumn);
    Checks.name(targetColumn);
    Checks.name(sourceLabel);
    Checks.name(targetLabel);
    properties = Checks.properties(properties);
  }

  public static EdgeMapping edge(
      String label,
      Source source,
      String id,
      String from,
      String to,
      String fromLabel,
      String toLabel) {
    return new EdgeMapping(label, source, id, from, to, fromLabel, toLabel, Map.of());
  }

  public EdgeMapping property(String name, String column) {
    var p = new TreeMap<>(properties);
    if (p.putIfAbsent(Checks.name(name), Checks.name(column)) != null)
      throw new IllegalArgumentException("Duplicate property " + name);
    return new EdgeMapping(
        label, source, id, sourceColumn, targetColumn, sourceLabel, targetLabel, p);
  }
}
