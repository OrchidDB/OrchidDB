package io.orchiddb;

import java.util.Objects;

/**
 * A caller-maintained, flat permission relation. The relation must contain effective grants
 * (including inherited/group grants) for the configured resource permission.
 */
public record PermissionRelation(
    Source source,
    String resourceType,
    String permission,
    String resourceTypeColumn,
    String permissionColumn,
    String resourceIdColumn,
    String subjectTypeColumn,
    String subjectRelationColumn,
    String subjectIdColumn) {
  public PermissionRelation(
      Source source,
      String resourceType,
      String permission,
      String resourceTypeColumn,
      String permissionColumn,
      String resourceIdColumn,
      String subjectTypeColumn,
      String subjectIdColumn) {
    this(
        source,
        resourceType,
        permission,
        resourceTypeColumn,
        permissionColumn,
        resourceIdColumn,
        subjectTypeColumn,
        "subject_rel",
        subjectIdColumn);
  }

  public PermissionRelation {
    Objects.requireNonNull(source);
    Checks.name(resourceType);
    Checks.name(permission);
    Checks.name(resourceTypeColumn);
    Checks.name(permissionColumn);
    Checks.name(resourceIdColumn);
    Checks.name(subjectTypeColumn);
    Checks.name(subjectRelationColumn);
    Checks.name(subjectIdColumn);
  }

  /** Flat relation using the conventional resource and subject column names. */
  public static PermissionRelation flat(Source source, String resourceType, String permission) {
    return new PermissionRelation(
        source,
        resourceType,
        permission,
        "resource_type",
        "resource_rel",
        "resource_id",
        "subject_type",
        "subject_rel",
        "subject_id");
  }

  /** Flat relation with caller-selected columns, independent of the permission system. */
  public static PermissionRelation flat(
      Source source,
      String resourceType,
      String permission,
      String resourceTypeColumn,
      String permissionColumn,
      String resourceIdColumn,
      String subjectTypeColumn,
      String subjectRelationColumn,
      String subjectIdColumn) {
    return new PermissionRelation(
        source,
        resourceType,
        permission,
        resourceTypeColumn,
        permissionColumn,
        resourceIdColumn,
        subjectTypeColumn,
        subjectRelationColumn,
        subjectIdColumn);
  }
}
