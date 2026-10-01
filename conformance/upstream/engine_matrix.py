#!/usr/bin/env python3
"""Run all three original language suites independently on each SQL backend.

Uses the standard run.py adapters, fixtures, catalog and assertions. Configure
the native/JVM binaries as documented in conformance/README.md. PostgreSQL
connections are supplied in ORCHIDDB_TEST_PG_URL; no database objects are created
on that connection by the SQL region executor.
"""
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SUITES = ("opencypher", "tinkerpop", "rdf")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", choices=("duckdb", "postgres"), action="append")
    parser.add_argument("--suite", choices=SUITES, action="append")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/conformance/sql-engines")
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    summaries = []
    failed = False
    for engine in args.engine or ("duckdb", "postgres"):
        config = {"dialect": engine}
        if engine == "postgres":
            config["connection"] = os.environ["ORCHIDDB_TEST_PG_URL"]
        env = {**os.environ, "ORCHIDDB_SQL_ENGINE_JSON": json.dumps(config)}
        for suite in args.suite or SUITES:
            output = (args.output_dir / f"{engine}-{suite}.json").resolve()
            subprocess.run([sys.executable, str(Path(__file__).with_name("run.py")),
                            "--engine", "orchiddb", "--suite", suite,
                            "--output", str(output)], cwd=ROOT, env=env, check=True)
            report = json.loads(output.read_text())
            counts = Counter(row["status"] for row in report["results"])
            full = (not report["coverage"]["filtered"] and
                    report["coverage"]["recorded_cases"] == report["coverage"]["catalog_cases"])
            wrong_engine = "postgres" if engine == "duckdb" else "duckdb"
            wrong_regions = sum(
                query.get("cost", {}).get("sql_regions", {}).get(wrong_engine, 0)
                for row in report["results"]
                for query in row.get("query_cost", {}).get("queries", []))
            passed = full and not wrong_regions and not (set(counts) - {"pass", "skipped", "not-applicable"})
            failed |= not passed
            summary = dict(engine=engine, suite=suite, counts=dict(counts),
                           full_catalog=full, other_engine_regions=wrong_regions,
                           success=passed, report=str(output))
            summaries.append(summary)
            print(json.dumps(summary), flush=True)
    (args.output_dir / "summary.json").write_text(json.dumps(summaries, indent=2) + "\n")
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
