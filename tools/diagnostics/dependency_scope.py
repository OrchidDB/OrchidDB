#!/usr/bin/env python3
"""Measure active normal/build dependency graphs without counting historical caches."""
import argparse
import json
import pathlib
import re
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
REGISTRY = pathlib.Path.home() / ".cargo" / "registry"
PROFILES = {
    "duckdb": ["-p", "orchiddb"],
    "trino": ["-p", "orchiddb", "--no-default-features", "--features", "trino"],
    "compiler": ["-p", "orchiddb", "--no-default-features"],
    "extension": ["-p", "orchid-duckdb-compiler"],
}
EXECUTION_CRATES = {
    "datafusion", "datafusion-execution", "datafusion-physical-plan",
    "datafusion-physical-optimizer", "datafusion-session", "datafusion-catalog",
    "datafusion-catalog-listing", "datafusion-datasource", "datafusion-datasource-arrow",
    "datafusion-datasource-csv", "datafusion-datasource-json", "datafusion-common-runtime",
}


def measure(arguments):
    command = ["cargo", "tree", "--locked", "--edges", "normal,build", "--prefix", "none", "--format", "{p}", *arguments]
    tree = subprocess.check_output(command, cwd=ROOT, text=True)
    packages = {}
    for line in tree.splitlines():
        match = re.match(r"^([\w-]+) v([^ ]+)(?: \(([^)]+)\))?", line)
        if not match:
            continue
        name, version, location = match.groups()
        if name in ("orchiddb", "orchid-duckdb-compiler"):
            continue
        key = f"{name}@{version}"
        archives = list((REGISTRY / "cache").glob(f"*/{name}-{version}.crate"))
        packages[key] = {
            "name": name,
            "version": version,
            # Count the same upstream archive for patched/vendor packages: moving
            # a dependency into this repository is not a footprint reduction.
            "upstream_archive_bytes": archives[0].stat().st_size if archives else None,
            "local_path": location if location and location != "*" else None,
        }
    return {
        "command": command,
        "package_count": len(packages),
        "datafusion_packages": sorted(k for k in packages if k.startswith("datafusion")),
        "upstream_archive_bytes": sum(p["upstream_archive_bytes"] or 0 for p in packages.values()),
        "packages_without_archive": sorted(k for k, p in packages.items() if p["upstream_archive_bytes"] is None),
        "packages": packages,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--artifact", type=pathlib.Path)
    parser.add_argument("--compare", type=pathlib.Path)
    parser.add_argument("--assert-compiler-only", action="store_true")
    parser.add_argument("--archive", type=pathlib.Path)
    args = parser.parse_args()
    report = {"scope": "active normal/build dependency closure; no dev dependencies or accumulated caches", "profiles": {k: measure(v) for k, v in PROFILES.items()}}
    if args.artifact:
        report["artifact"] = {"path": str(args.artifact), "bytes": args.artifact.stat().st_size}
    if args.archive:
        report["compiler_archive"] = {"path": str(args.archive), "bytes": args.archive.stat().st_size}
    if args.assert_compiler_only:
        forbidden = {name: sorted(p["name"] for p in profile["packages"].values() if p["name"] in EXECUTION_CRATES) for name, profile in report["profiles"].items()}
        forbidden = {name: packages for name, packages in forbidden.items() if packages}
        if forbidden:
            raise SystemExit(f"DataFusion execution dependencies reintroduced: {forbidden}")
        report["compiler_only_dependency_check"] = "passed"
    if args.compare:
        before = json.loads(args.compare.read_text())
        report["comparison"] = {}
        for name, after in report["profiles"].items():
            old = before["profiles"][name]
            report["comparison"][name] = {
                "before_count": old["package_count"], "after_count": after["package_count"],
                "removed": sorted(old["packages"].keys() - after["packages"].keys()),
                "added": sorted(after["packages"].keys() - old["packages"].keys()),
                "before_archive_bytes": old["upstream_archive_bytes"], "after_archive_bytes": after["upstream_archive_bytes"],
            }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: {"packages": v["package_count"], "archive_bytes": v["upstream_archive_bytes"]} for k, v in report["profiles"].items()}, indent=2))


if __name__ == "__main__":
    main()
