#!/usr/bin/env python3
"""Rank the relational conformance harness's slow-case traces as TSV.

Example: scripts/slow-conformance-queries.py target/coverage-reports/*/shard-*.log
Missing phase timings mean no trace was emitted (usually below GRAPH_REL_SLOW_MS),
not zero time. Total time includes fixture setup, planning, and execution.
"""

import argparse
import csv
import re
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logs", nargs="+", type=Path)
    parser.add_argument("--min-ms", type=int, default=500)
    args = parser.parse_args()
    case_pattern = re.compile(
        r"slow-case elapsed_ms=(\d+) outcome=(\S+) plan_lines=(\d+) "
        r"sql_bytes=(\d+) case=(\S+) query=(.*)"
    )
    phase_pattern = re.compile(
        r"slow-phase elapsed_ms=(\d+) phase=(\S+) case=(\S+)"
    )
    rows = []
    for log in args.logs:
        phases = {}
        with log.open() as source:
            for line in source:
                if match := phase_pattern.search(line):
                    ms, phase, path = match.groups()
                    phases.setdefault(path, {})[phase] = ms
                if match := case_pattern.search(line):
                    ms, outcome, plan_lines, sql_bytes, path, query = match.groups()
                    timing = phases.pop(path, {})
                    if int(ms) >= args.min_ms:
                        rows.append([
                            int(ms), timing.get("dataset", ""),
                            timing.get("plan", ""),
                            timing.get("lower_prepare_execute", ""),
                            outcome, plan_lines, sql_bytes, path, query, str(log),
                        ])
    writer = csv.writer(sys.stdout, delimiter="\t", lineterminator="\n")
    writer.writerow([
        "elapsed_ms", "dataset_ms", "plan_ms", "query_pipeline_ms", "outcome",
        "plan_lines", "sql_bytes", "case", "query", "source_log",
    ])
    writer.writerows(sorted(rows, key=lambda row: (-row[0], row[7])))


if __name__ == "__main__":
    main()
