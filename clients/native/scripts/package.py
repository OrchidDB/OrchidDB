#!/usr/bin/env python3
"""Package the current platform's compiler library and ABI metadata."""
import argparse
import hashlib
import json
import re
import tarfile
import tempfile
from pathlib import Path
from smoke import load, metadata

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--library", type=Path, required=True)
parser.add_argument("--target", required=True, choices=[
    "x86_64-unknown-linux-gnu", "aarch64-apple-darwin",
    "x86_64-apple-darwin", "aarch64-unknown-linux-gnu"])
parser.add_argument("--output", type=Path, default=ROOT / "dist")
parser.add_argument("--tag", required=True)
args = parser.parse_args()
meta = metadata(load(args.library))
assert args.tag == "v" + meta["version"], "tag must match native package version"
assert re.fullmatch(r"[0-9a-f]{40}", meta["core_revision"]), "release source must be clean and identified"
assert meta["core_revision"] == (ROOT / "CORE_REVISION").read_text().strip(), "core revision differs from release pin"
expected = "orchiddb_compiler.dll" if "windows" in args.target else (
    "liborchiddb_compiler.dylib" if "apple" in args.target else "liborchiddb_compiler.so")
assert args.library.name == expected, "library filename must match target"
meta.update(target=args.target, library="lib/" + expected,
            sha256=hashlib.sha256(args.library.read_bytes()).hexdigest())
args.output.mkdir(parents=True, exist_ok=True)
archive = args.output / f"orchiddb-compiler-{args.tag}-{args.target}.tar.gz"
with tempfile.TemporaryDirectory() as tmp:
    manifest = Path(tmp) / "manifest.json"
    manifest.write_text(json.dumps(meta, indent=2, sort_keys=True) + "\n")
    with tarfile.open(archive, "w:gz") as tar:
        tar.add(args.library, arcname=meta["library"])
        tar.add(ROOT / "include/orchiddb.h", arcname="include/orchiddb.h")
        tar.add(ROOT / "LICENSE.md", arcname="LICENSE.md")
        tar.add(manifest, arcname="manifest.json")
archive.with_name(archive.name + ".sha256").write_text(
    hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\n")
print(archive)
