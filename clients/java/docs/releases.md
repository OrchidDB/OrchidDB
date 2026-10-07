# JVM releases on GitHub

All Java source and release helpers now live in `clients/java/` in the main
OrchidDB repository. See [source-build instructions](../../README.md). The source
import does not publish new binary packages. The following describes the ZIP
format produced by `scripts/package-github.py` for locally validated builds.
Use Java 17 or newer.

## Install the distribution

Verify the ZIP against `SHA256SUMS`, then extract it into a directory such as
`orchiddb-java-0.3.0/`. On macOS or Linux:

```sh
shasum -a 256 -c SHA256SUMS
unzip orchiddb-java-0.3.0.zip -d orchiddb-java-0.3.0
java -cp "orchiddb-java-0.3.0/lib/*:your-application.jar" your.Main
```

The distribution contains:

| Path | Contents |
| --- | --- |
| `lib/` | Java and Gremlin clients, their runtime dependencies, and all four native classifier JARs |
| `repository/com/orchiddb/` | Standard Maven layout with parent/module POMs, main JARs, native classifiers, sources, Javadocs, and checksums |
| `release-manifest.json` | Client commit, pinned core commit, version, and SHA256 for each packaged file |
| `README.md`, `docs/` | Usage and integration documentation |
| `LICENSE.md` | GPL-3.0 license |

The compiler loader selects the native classifier for the current JVM, verifies
its version, core revision and checksum, then extracts and loads it. It makes no
network request and requires no separate native build:

```java
var compiler = io.orchiddb.NativeSqlCompiler.load();
```

Supply your own JDBC driver. The bundle includes the clients' declared runtime
dependencies; Arrow execution also requires an application-selected memory
implementation and driver exporter. See the [Arrow guide](arrow.md).

## Use the extracted Maven repository

The repository is a normal Maven file repository. Add its absolute URL to your
application's POM:

```xml
<repositories>
  <repository>
    <id>orchiddb-github</id>
    <url>file:///absolute/path/orchiddb-java-0.3.0/repository</url>
  </repository>
</repositories>
<dependencies>
  <dependency>
    <groupId>com.orchiddb</groupId>
    <artifactId>orchiddb-java</artifactId>
    <version>0.3.0</version>
  </dependency>
  <dependency>
    <groupId>com.orchiddb</groupId>
    <artifactId>orchiddb-java</artifactId>
    <version>0.3.0</version>
    <classifier>macos-aarch64</classifier>
    <scope>runtime</scope>
  </dependency>
</dependencies>
```

Replace the URL with your extracted directory. Alternatively, copy the
`repository/com/orchiddb/` subtree into `~/.m2/repository/com/orchiddb/`.
Third-party dependencies continue to resolve through your usual Maven repositories.

Choose exactly one classifier for the JVM running your application:

| JVM platform | Native classifier |
| --- | --- |
| Linux x86-64 | `linux-x86_64` |
| macOS ARM64 / Apple Silicon | `macos-aarch64` |
| macOS x86-64 / Intel | `macos-x86_64` |
| Linux ARM64 | `linux-aarch64` |

For TinkerPop integration, add `com.orchiddb:orchiddb-gremlin:0.3.0` and retain the
native dependency. Keep the Java, Gremlin and native classifier versions aligned.
The parent coordinate is `com.orchiddb:orchiddb-parent:0.3.0`; consumers normally
resolve it through the module POMs without declaring it directly.

## Prepare and package a release

Follow the coordinated process in the core repository's
[`scripts/release/README.md`](https://github.com/OrchidDB/OrchidDB/blob/main/scripts/release/README.md).
Run all runtime, integration, and packaged-classpath tests locally. Build and
package locally with `make release VERSION=X.Y.Z` from the workspace; GitHub
receives the finished release assets only.

Generate `native/CORE_REVISION` from the clean monorepo build, align the parent and child
Maven versions, commit the sources, and push the immutable matching version tag.
The local release builder produces Linux ARM64/x86_64 and macOS ARM64/x86_64
classifiers and reuses completed artifacts. Windows is excluded.

`scripts/verify-native-artifacts.py` checks the four classifiers' version, core
revision, license, and checksums. The packager includes source/Javadoc JARs,
the standard Maven repository, runtime classpath, and per-file manifest.

Upload these completed files to the matching GitHub release. Do not move a published
tag or replace published release files with different bytes. No Central account,
Sonatype credentials, signing service, or Maven registry deployment is involved.

## Source development

For local development, `-Dorchiddb.native.path=/absolute/path/to/compiler` overrides
classpath extraction, or call `NativeSqlCompiler.load(Path)` explicitly. A local
build does not certify other platforms. Use the completed release classifiers for
the published distribution.
