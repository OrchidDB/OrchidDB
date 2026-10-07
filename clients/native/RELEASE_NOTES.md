OrchidDB compiler C ABI 1 for caller-owned SQL engines.

Includes the versioned JSON compiler boundary, ownership-safe response release,
package/core version metadata, a public C header, and native platform libraries.
No DuckDB driver is linked. Result data stays in the caller's Arrow execution path.

Archives contain a manifest and library checksum. Match the release to the client
package's native pin; review the included license and supported target platform.
