package io.orchiddb.internal;

/** Internal versioned compiler and optional HTTP transport boundary. */
public final class NativeBridge {
  private NativeBridge() {}

  public static native String compileJson(String request);

  public static native String statisticsJson(String request);

  public static native String remoteJson(String request);
}
