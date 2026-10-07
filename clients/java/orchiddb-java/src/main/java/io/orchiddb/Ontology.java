package io.orchiddb;

import java.util.*;

public record Ontology(
    List<ClassMapping> classes,
    List<PropertyMapping> properties,
    List<RelationshipMapping> relationships) {
  public static final Ontology EMPTY = new Ontology(List.of(), List.of(), List.of());

  public Ontology {
    classes = List.copyOf(classes);
    properties = List.copyOf(properties);
    relationships = List.copyOf(relationships);
  }

  public record ClassMapping(String iri, String label, String identity) {
    public ClassMapping {
      Checks.name(iri);
      Checks.name(label);
    }
  }

  public record PropertyMapping(String iri, String label, String property) {
    public PropertyMapping {
      Checks.name(iri);
      Checks.name(label);
      Checks.name(property);
    }
  }

  public record RelationshipMapping(
      String iri, String label, String source_label, String target_label) {
    public RelationshipMapping {
      Checks.name(iri);
      Checks.name(label);
      Checks.name(source_label);
      Checks.name(target_label);
    }
  }
}
