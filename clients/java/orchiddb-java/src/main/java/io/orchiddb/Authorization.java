package io.orchiddb;

import java.util.Objects;

/** Principal for one query. It is included in the immutable compilation/cache key. */
public record Authorization(String subjectType, String subjectId) {
  public Authorization {
    Checks.name(subjectType);
    Objects.requireNonNull(subjectId);
    if (subjectId.isEmpty()) throw new IllegalArgumentException("subjectId must not be empty");
  }
}
