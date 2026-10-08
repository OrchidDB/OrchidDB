package io.orchiddb;

import java.nio.file.Path;
import java.util.Map;
import java.util.Objects;

public final class Credential {
  private final String source;
  private final String value;

  private Credential(String source, String value) {
    this.source = source;
    this.value = Objects.requireNonNull(value);
    if (value.isEmpty()) throw new IllegalArgumentException("Credential cannot be empty");
  }

  public static Credential value(String value) { return new Credential("value", value); }
  public static Credential environment(String name) { return new Credential("env", name); }
  public static Credential file(Path path) { return new Credential("file", path.toString()); }
  Map<String, String> configuration() { return Map.of("source", source, "value", value); }
  @Override public String toString() { return "Credential([redacted])"; }
}
