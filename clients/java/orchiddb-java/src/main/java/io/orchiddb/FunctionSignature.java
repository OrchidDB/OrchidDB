package io.orchiddb;

import java.util.*;

/** Describes a function already installed by the caller; never installs or executes it. */
public record FunctionSignature(
    String name, String target, List<String> parameters, String returns, boolean aggregate) {
  public FunctionSignature {
    Checks.name(name);
    Checks.name(target);
    parameters = List.copyOf(parameters);
    parameters.forEach(Checks::name);
    Checks.name(returns);
  }

  public static FunctionSignature scalar(
      String name, String target, String returns, String... parameters) {
    return new FunctionSignature(name, target, List.of(parameters), returns, false);
  }
}
