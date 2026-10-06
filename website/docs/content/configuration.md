# Configuration

Configure the connection, credentials, memory, threads, and source extensions with
DuckDB's ordinary settings. Load Orchid before native graph statements. Development
artifacts require `allow_unsigned_extensions=true` and a matching DuckDB version.

JVM kernels use the existing `jvm` module and `ORCHIDDB_JVM_CLASSPATH` configuration.
See the repository's [JVM guide](https://github.com/OrchidDB/OrchidDB/blob/main/docs/jvm.md)
for internal build details. The `orchiddb-jvm-store` helper is internal kernel
infrastructure, not the removed public CLI.

No Orchid server, client SDK configuration, or PostgreSQL executor is required.
