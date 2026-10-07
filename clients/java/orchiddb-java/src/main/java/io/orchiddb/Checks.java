package io.orchiddb;

import java.util.*;

final class Checks {
  static String name(String name) {
    Objects.requireNonNull(name);
    if (name.isBlank() || name.indexOf(0) >= 0)
      throw new IllegalArgumentException("Names must be nonblank and contain no NUL");
    return name;
  }

  static Map<String, String> properties(Map<String, String> properties) {
    var copy = new TreeMap<String, String>();
    properties.forEach((k, v) -> copy.put(name(k), name(v)));
    return Collections.unmodifiableMap(copy);
  }
}
