# Coordinated releases

Run runtime tests locally. GitHub Actions only builds, packages, signs, and
publishes artifacts. Release workflows are manual: pushing a tag does not start
another matrix. Dispatch the audited workflow on `main` with the immutable
source tag as its input; an old tag can contain obsolete workflow instructions.

`pipeline.py` coordinates all nine sibling checkouts, records exact source
revisions and local test logs, and resumes builds without repeating completed
work. It requires Python 3.11+, authenticated `gh`, Git, and the client toolchains.
The workspace contains `orchiddb`, `orchiddb-native`, `orchiddb-rust`,
`orchiddb-cli`, `orchiddb-java`, `orchiddb-python`, `orchiddb-js`,
`orchiddb-elixir`, and `orchiddb-cpp`.

## Prepare and test locally

1. Update versions consistently. Commit the engine first, then native/JNI and
   Rust pins, then foreign-client native pins and the CLI Rust-client pin.
   Commit and push intended changes in all repositories. Keep published tags
   immutable; corrections to published package bytes require a new version.
2. Install local prerequisites: Rust 1.93.1, JDK 21/Maven, Python with venv,
   Node/npm, Erlang/Elixir, CMake, a C++ compiler, and PostgreSQL. Set
   `ORCHIDDB_TEST_PG_URL` to a working local PostgreSQL database; also set the
   corresponding `ORCHIDDB_TEST_PG_URI` and `ORCHIDDB_TEST_PG_JDBC` for the
   foreign clients. Set `DUCKDB_LIB_DIR` and the operating system's library
   search path when using an external DuckDB 1.5.2 installation. Without it,
   local Rust integration builds use bundled DuckDB.
3. From the workspace root, substitute the intended version below:

```sh
python3 orchiddb/scripts/release/pipeline.py quota --version X.Y.Z
python3 orchiddb/scripts/release/pipeline.py plan --version X.Y.Z
python3 orchiddb/scripts/release/pipeline.py test --version X.Y.Z
```

The committed `integration.mk` runs workspace, Java/Gremlin, Rust, CLI, shared
native, Python, Elixir, JavaScript, and C++ checks. Core tests run separately.
A failed local check stops the release before any cloud build. Successful checks
are reused only for the same source revisions and unchanged test-log hashes.
State and logs live in `.releases/X.Y.Z/`; retain this directory across sessions.

The quota preflight refreshes warnings from a previous Maven deployment when
local Central credentials are available. Preserve `maven-deployment.json` in the
release directory. Without a previous deployment report, check Central account
usage before starting builds. For the 0.2.0 release, Central reported a block on
subsequent publication until November 1, 2026; 0.2.0 itself was published.

## Build and resume

```sh
python3 orchiddb/scripts/release/pipeline.py build --version X.Y.Z
python3 orchiddb/scripts/release/pipeline.py status --version X.Y.Z
python3 orchiddb/scripts/release/pipeline.py collect --version X.Y.Z
python3 orchiddb/scripts/release/pipeline.py build --version X.Y.Z
```

`build` audits both local and actual remote workflow definitions, creates/pushes
immutable tags, and dispatches each missing producer/platform once. It saves a
request identity before dispatch and finds the same request after interruption.
An uncertain network outcome is not retried blindly. `status` shows each run's
state, elapsed time, and URL. Collect artifacts as jobs finish, then run `build`
again to start client packaging once every shared compiler is available.

| Artifact | Linux x86_64 | macOS ARM64 | macOS x86_64 | Windows x86_64 |
| --- | --- | --- | --- | --- |
| Shared native compiler | Yes | Yes | Yes | Yes |
| Java JNI classifier | Yes | Yes | Yes | Yes |
| CLI | Yes | Yes | Yes | — |
| Python wheel | Yes | Yes | Yes | Yes |
| JavaScript / C++ | Yes | Yes | Yes | — |

Mac release jobs use GitHub's `macos-26` (ARM64) and `macos-26-intel`
(Intel) images and their Apple build tools. Native, JNI, and CLI builds retain
`MACOSX_DEPLOYMENT_TARGET=11.0`; the runner OS is not the minimum supported OS.
Rust remains pinned to 1.93.1 across release producers. The collector accepts
both the new Python/C++ artifact names and the older `macos-14` / `macos-15-intel`
names, so completed pre-upgrade runs remain reusable.

Python, JavaScript, and C++ reuse the shared compiler's verified artifacts.
JNI and the CLI have separate binaries. The CLI uses official, checksum-pinned
DuckDB static libraries, avoiding a fresh DuckDB C++ compilation on each runner.
Rust registry packages are prepared together once; Elixir is a source package.
Caches survive failed jobs. Tags and ordinary pushes do not trigger duplicate
release builds. Pull-request workflows are build-only.

For failed work:

```sh
python3 orchiddb/scripts/release/pipeline.py retry --version X.Y.Z
```

This first collects available artifacts, including artifacts from failed runs,
then retries failed work without completed outputs. Running and successful jobs
are retained. Producer retries select one platform. Client packaging retries can
repeat packaging jobs, but cannot recompile the shared native compiler. Never
cancel an entire matrix because one platform is slow. Investigate the specific
run and report its elapsed time and failing step.

## Verify and publish

```sh
python3 orchiddb/scripts/release/pipeline.py verify --version X.Y.Z
```

This requires every platform, source provenance, matching versions, native
architectures and relevant embedded checksums. It detects modifications to
collected files and writes `verified-release.json` with source revisions, local
test evidence, run IDs, and artifact hashes. The coordinator stops at this
verified handoff; it does not automatically publish registries or GitHub releases.

Publish from these collected artifacts, preserving the handoff and checksums:

- Create draft GitHub releases for all nine repositories. The existing
  `release.py draft --tag vX.Y.Z --directory ABSOLUTE_ARTIFACT_DIRECTORY` helper
  creates checksums/manifests and refuses to replace a public release. Run it
  in an isolated checkout at the matching tag. Combine each product's platform
  artifacts first, checking duplicate filenames rather than overwriting them.
  Engine/client `.crate` archives come from the single Rust producer.
- Publish the engine crate before the Rust client. Publish the collected npm
  tarball and Python wheels. Python's `release.yml` accepts `release_id` with
  `publish=true` to upload an existing checksummed draft without rebuilding;
  dispatch it from `main`. Already published PyPI files must match exactly.
  Publish Hex from the tagged source and compare its archive with the collected
  package. Preserve registry receipts and compare downloaded package hashes.
- For Maven, dispatch Java `release.yml` from `main` with `tag=vX.Y.Z`,
  `platform=all`, `stage=true`, `publish=false`, and `reuse_runs` containing all
  four recorded Java build run IDs. It reuses every classifier, signs and stages
  with tests skipped, and records the Central deployment ID. Save that ID and
  status as `maven-deployment.json`; publish that deployment once validated.
  Do not rerun the native build matrix to stage Maven.
- Include a Python source distribution if distributing source: build it locally
  from the immutable tag, record its hash, and add it to the Python draft before
  upload. Classifier JARs are intermediate inputs; publish Maven's signed module
  artifacts and the completed deployment evidence with the Java release.
- Verify public registry file hashes and GitHub asset digests, attach validation
  evidence, then publish the GitHub drafts using `gh release edit vX.Y.Z
  --draft=false --repo OrchidDB/REPOSITORY`. Never replace published bytes.

Use the 0.2.0 GitHub release's `release-manifest.json`, registry verification,
artifact verification, workflow audit, and evidence checksums as the evidence
format. Record known failures candidly: 0.2.0 had one intermittent Elixir ADBC
rollback failure during downloaded-artifact validation; subsequent repeated
checks passed, but its root cause was not established.

## Check changes to the process

Run these locally, never from Actions:

```sh
python3 -m unittest discover -s orchiddb/scripts/release -p 'test_*.py'
python3 orchiddb-cli/scripts/release/test_build.py
python3 orchiddb-java/scripts/test-package-line-endings.py
python3 orchiddb/scripts/release/pipeline.py audit --version X.Y.Z
```

The audit catches known test commands, automatic tag builds, and Maven builds
without `-DskipTests`. It is a regression guard, not a shell interpreter: review
new scripts and third-party actions as well. Keep `AGENTS.md` in every repository
so future release work retains the local-test and artifact-reuse requirements.
