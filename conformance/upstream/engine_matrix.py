#!/usr/bin/env python3
"""Run the original Cypher suite independently on each SQL source dialect.

Uses the standard run.py adapters, fixtures, catalog and assertions. Configure
the native binary as documented in the root README. Trino uses the Orchid
Arrow connector and ORCHIDDB_TEST_TRINO_URL / ORCHIDDB_EXCHANGE_TOKEN.
"""
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SUITES = ("opencypher",)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", choices=("duckdb", "trino"), action="append")
    parser.add_argument("--suite", choices=SUITES, action="append")
    parser.add_argument("--jobs", type=int, default=4, help="Cypher workers per backend")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/conformance/sql-engines")
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    summaries = []
    failed = False
    for engine in args.engine or ("duckdb", "trino"):
        config = {"dialect": engine}
        if engine == "trino":
            config.update(endpoint=os.environ["ORCHIDDB_TEST_TRINO_URL"],
                          exchange_token=os.environ["ORCHIDDB_EXCHANGE_TOKEN"],
                          exchange_bind=os.environ.get("ORCHIDDB_EXCHANGE_BIND", "0.0.0.0:18883"),
                          exchange_catalog=os.environ.get("ORCHIDDB_EXCHANGE_CATALOG", "orchid"))
        env = {**os.environ, "ORCHIDDB_SQL_ENGINE_JSON": json.dumps(config)}
        binary = Path(env.get('CONFORMANCE_ORCHIDDB_BINARY', ROOT / 'target/debug/upstream'))
        library_path = 'DYLD_LIBRARY_PATH' if sys.platform == 'darwin' else 'LD_LIBRARY_PATH'
        env[library_path] = str(binary.parent / 'deps') + (os.pathsep + env[library_path] if env.get(library_path) else '')
        for suite in args.suite or SUITES:
            output = (args.output_dir / f"{engine}-{suite}.json").resolve()
            subprocess.run([sys.executable, str(Path(__file__).with_name("run.py")),
                            "--engine", "orchiddb", "--suite", suite,
                            "--jobs", str(1 if engine == "trino" else args.jobs),
                            "--output", str(output)], cwd=ROOT, env=env, check=True)
            report = json.loads(output.read_text())
            counts = Counter(row["status"] for row in report["results"])
            full = (not report["coverage"]["filtered"] and
                    report["coverage"]["recorded_cases"] == report["coverage"]["catalog_cases"])
            wrong_regions = sum(
                sum(count for name, count in (query.get("cost") or {}).get("sql_regions", {}).items() if name != engine)
                for row in report["results"]
                for query in row.get("query_cost", {}).get("queries", []))
            engine_regions = sum(
                (query.get("cost") or {}).get("sql_regions", {}).get(engine, 0)
                for row in report["results"]
                for query in row.get("query_cost", {}).get("queries", []))
            unexpected = [row['id'] for row in report['results'] if row['status'] != 'pass']
            single_instance = True
            passed = full and single_instance and engine_regions > 0 and not wrong_regions and not unexpected
            failed |= not passed
            summary = dict(engine=engine, suite=suite, counts=dict(counts),
                           full_catalog=full, engine_regions=engine_regions, other_engine_regions=wrong_regions,
                           single_instance_verified=single_instance, unexpected_cases=unexpected,
                           success=passed, report=str(output))
            summaries.append(summary)
            print(json.dumps(summary), flush=True)
    (args.output_dir / "summary.json").write_text(json.dumps(summaries, indent=2) + "\n")
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
