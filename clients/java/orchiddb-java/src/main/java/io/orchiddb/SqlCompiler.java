package io.orchiddb;

@FunctionalInterface
public interface SqlCompiler {
  CompiledQuery compile(Compilation request);

  /** Shared statistics protocol, also usable with application-owned execution sessions. */
  default String statisticsCommand(String command) {
    throw new UnsupportedOperationException("This compiler does not support statistics");
  }
}
