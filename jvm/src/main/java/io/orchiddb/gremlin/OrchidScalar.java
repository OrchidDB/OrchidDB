package io.orchiddb.gremlin;

import java.util.*;

/** A lossless Arrow scalar for source types without a Java primitive equivalent. */
public final class OrchidScalar {
    private final Map<String,Object> payload;
    public OrchidScalar(Map<String,Object> payload) {
        Map<String,Object> copy = new LinkedHashMap<>(payload);
        copy.put("bytes", List.copyOf((List<?>)payload.get("bytes")));
        this.payload = Collections.unmodifiableMap(copy);
    }
    public String dataType() { return payload.get("data_type").toString(); }
    Map<String,Object> payload() { return payload; }
    @Override public String toString() { return payload.get("display").toString(); }
    @Override public boolean equals(Object other) { return other instanceof OrchidScalar scalar && payload.get("bytes").equals(scalar.payload.get("bytes")); }
    @Override public int hashCode() { return payload.get("bytes").hashCode(); }
}
