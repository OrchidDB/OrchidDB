package io.orchiddb.internal;

import io.orchiddb.PlanningException;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;
import java.util.Locale;
import java.util.Properties;

/** Resolves only explicitly installed classpath artifacts; never downloads a binary. */
public final class NativeLibrary {
  private static Path extracted;

  private NativeLibrary() {}

  public static String platform(String osName, String osArch) {
    String os = osName.toLowerCase(Locale.ROOT);
    String arch = osArch.toLowerCase(Locale.ROOT);
    String cpu =
        switch (arch) {
          case "amd64", "x86_64" -> "x86_64";
          case "aarch64", "arm64" -> "aarch64";
          default -> throw new PlanningException("Unsupported JVM architecture: " + osArch);
        };
    String system;
    if (os.startsWith("mac") || os.equals("darwin")) system = "macos";
    else if (os.startsWith("windows")) system = "windows";
    else if (os.equals("linux")) system = "linux";
    else throw new PlanningException("Unsupported JVM operating system: " + osName);
    return system + "-" + cpu;
  }

  public static synchronized Path extract() {
    if (extracted != null) return extracted;
    String platform = platform(System.getProperty("os.name"), System.getProperty("os.arch"));
    String root = "/io/orchiddb/native/" + platform + "/";
    String name =
        platform.startsWith("windows")
            ? "orchiddb_java.dll"
            : platform.startsWith("macos") ? "liborchiddb_java.dylib" : "liborchiddb_java.so";
    Path dir = null;
    try {
      var metadata = new Properties();
      try (var stream = NativeLibrary.class.getResourceAsStream(root + "build.properties")) {
        if (stream == null)
          throw new PlanningException(
              "Missing OrchidDB native artifact for "
                  + platform
                  + "; add com.orchiddb:orchiddb-java with classifier "
                  + platform
                  + " at the same version, or use NativeSqlCompiler.load(Path)");
        metadata.load(stream);
      }
      // A dedicated resource also works in shaded/fat JARs whose manifest belongs to the app.
      var api = new Properties();
      try (var stream = NativeLibrary.class.getResourceAsStream("/io/orchiddb/build.properties")) {
        if (stream == null) throw new PlanningException("Java artifact is missing build metadata");
        api.load(stream);
      }
      String version = api.getProperty("version");
      if (version == null || !version.equals(metadata.getProperty("version")))
        throw new PlanningException(
            "Java/native artifact version mismatch; use matching packaged artifacts or load(Path)");
      try (var expected =
          NativeLibrary.class.getResourceAsStream("/io/orchiddb/native/CORE_REVISION")) {
        if (expected == null
            || !new String(expected.readAllBytes(), java.nio.charset.StandardCharsets.UTF_8)
                .trim()
                .equals(metadata.getProperty("coreRevision")))
          throw new PlanningException("Java/native compiler revision mismatch");
      }
      dir = Files.createTempDirectory("orchiddb-native-");
      Path library = dir.resolve(name);
      try (var stream = NativeLibrary.class.getResourceAsStream(root + name)) {
        if (stream == null)
          throw new PlanningException("Native artifact is incomplete: " + platform);
        Files.copy(stream, library);
      }
      var digest = MessageDigest.getInstance("SHA-256");
      try (var stream = Files.newInputStream(library)) {
        byte[] buffer = new byte[65536];
        for (int n; (n = stream.read(buffer)) != -1; ) digest.update(buffer, 0, n);
      }
      if (!HexFormat.of().formatHex(digest.digest()).equals(metadata.getProperty("sha256")))
        throw new PlanningException("Native artifact checksum mismatch");
      // Register parent first: the JVM executes delete-on-exit in reverse order.
      dir.toFile().deleteOnExit();
      library.toFile().deleteOnExit();
      extracted = library;
      return library;
    } catch (IOException | NoSuchAlgorithmException | RuntimeException e) {
      if (dir != null) {
        try {
          Files.deleteIfExists(dir.resolve(name));
          Files.deleteIfExists(dir);
        } catch (IOException suppressed) {
          e.addSuppressed(suppressed);
        }
      }
      if (e instanceof PlanningException p) throw p;
      throw new PlanningException(
          "Cannot extract OrchidDB native compiler; check java.io.tmpdir", e);
    }
  }
}
