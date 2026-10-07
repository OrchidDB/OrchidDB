package io.orchiddb;

import java.util.Objects;

/** A permission relation matched against a resource key on a mapped node. */
public record PermissionScope(String resourceColumn, PermissionRelation relation) {
  public PermissionScope {
    Checks.name(resourceColumn);
    Objects.requireNonNull(relation);
  }
}
