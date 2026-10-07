package io.orchiddb;

import java.util.*;

/**
 * Caches plans, never results. Keys include schema, dialect, mappings, functions and parameter
 * values.
 */
public interface PlanCache {
  CompiledQuery get(Compilation request);

  void put(Compilation request, CompiledQuery plan);

  static PlanCache none() {
    return new PlanCache() {
      public CompiledQuery get(Compilation r) {
        return null;
      }

      public void put(Compilation r, CompiledQuery p) {}
    };
  }

  static PlanCache bounded(int capacity) {
    if (capacity < 1) throw new IllegalArgumentException("capacity must be positive");
    return new PlanCache() {
      private final Map<Compilation, CompiledQuery> plans = new LinkedHashMap<>(16, .75f, true);

      public synchronized CompiledQuery get(Compilation r) {
        return plans.get(r);
      }

      public synchronized void put(Compilation r, CompiledQuery p) {
        plans.put(r, p);
        if (plans.size() > capacity) plans.remove(plans.keySet().iterator().next());
      }
    };
  }
}
