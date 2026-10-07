package io.orchiddb;

import java.util.*;

public record NodeMapping(
    String label,
    Source source,
    String id,
    Map<String, String> properties,
    List<PermissionScope> permissionScopes) {
  public NodeMapping(String label, Source source, String id, Map<String, String> properties) {
    this(label, source, id, properties, List.of());
  }

  /** Compatibility constructor: protect by matching the node identity column. */
  public NodeMapping(
      String label,
      Source source,
      String id,
      Map<String, String> properties,
      PermissionRelation permission) {
    this(
        label,
        source,
        id,
        properties,
        permission == null ? List.of() : List.of(new PermissionScope(id, permission)));
  }

  public NodeMapping {
    Checks.name(label);
    Objects.requireNonNull(source);
    Checks.name(id);
    properties = Checks.properties(properties);
    permissionScopes = List.copyOf(permissionScopes);
  }

  public static NodeMapping node(String label, Source source, String id) {
    return new NodeMapping(label, source, id, Map.of());
  }

  public NodeMapping property(String name, String column) {
    var p = new TreeMap<>(properties);
    if (p.putIfAbsent(Checks.name(name), Checks.name(column)) != null)
      throw new IllegalArgumentException("Duplicate property " + name);
    return new NodeMapping(label, source, id, p, permissionScopes);
  }

  /** Restrict every query returning this node label to the configured effective grants. */
  public NodeMapping protectWith(PermissionRelation relation) {
    Objects.requireNonNull(relation);
    return protectWith(id, relation);
  }

  /** Add an alternative grant whose resource ID matches a source column such as project_id. */
  public NodeMapping protectWith(String resourceColumn, PermissionRelation relation) {
    var scopes = new ArrayList<>(permissionScopes);
    scopes.add(new PermissionScope(resourceColumn, relation));
    return new NodeMapping(label, source, id, properties, scopes);
  }

  /** Compatibility accessor for mappings with exactly one permission scope. */
  public PermissionRelation permission() {
    return permissionScopes.isEmpty() ? null : permissionScopes.get(0).relation();
  }
}
