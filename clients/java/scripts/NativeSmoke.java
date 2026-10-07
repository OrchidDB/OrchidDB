import io.orchiddb.OrchidDB;

/** Verify packaged API/runtime loading without adding a database driver dependency. */
class NativeSmoke {
  public static void main(String[] args) {
    new OrchidDB();
    String schema = io.orchiddb.internal.NativeBridge.compileJson(
        "{\"op\":\"validate_schema\",\"schema\":{\"tables\":[]}}");
    if (!schema.contains("tables")) throw new AssertionError(schema);
    System.out.println("Packaged OrchidDB runtime loaded successfully.");
  }
}
