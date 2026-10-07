#!/usr/bin/env python3
"""Run all three original language suites independently on each SQL backend.

Uses the standard run.py adapters, fixtures, catalog and assertions. Configure
the native/JVM binaries as documented in the root README. PostgreSQL
connections are supplied in ORCHIDDB_TEST_PG_URL; no database objects are created
on that connection by the SQL region executor.
"""
import argparse
from collections import Counter
import gzip
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
    # Only the already-recorded SPARQL omissions are permitted. Bind the allowance
    # to each pinned case hash so newly changed or skipped cases cannot pass.
    with gzip.open(ROOT / 'conformance/extension-results/rdf.json.gz', 'rt') as stream:
        baseline = json.load(stream)
    omissions = {row['id']: (row['status'], row['case_sha256'])
                 for row in baseline['results']
                 if row['status'] in ('skipped', 'not-applicable')}
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
                (query.get("cost") or {}).get("sql_regions", {}).get(wrong_engine, 0)
                for row in report["results"]
                for query in row.get("query_cost", {}).get("queries", []))
            engine_regions = sum(
                (query.get("cost") or {}).get("sql_regions", {}).get(engine, 0)
                for row in report["results"]
                for query in row.get("query_cost", {}).get("queries", []))
            unexpected = [row['id'] for row in report['results']
                          if row['status'] != 'pass' and not (
                              suite == 'rdf' and omissions.get(row['id']) ==
                              (row['status'], row['case_sha256']))]
            single_instance = suite != 'tinkerpop' or report['execution_profile']['single_instance_verified']
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
