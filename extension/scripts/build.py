#!/usr/bin/env python3
"""Build the loadable extension locally against a pinned DuckDB release.

Uses the official CLI's exported C++ symbols, rather than embedding a second
DuckDB. Downloads are cached; Rust dependencies are pinned in Cargo.lock.
"""
import argparse
import hashlib
import os
from pathlib import Path
import platform
import subprocess
import tarfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
VERSION = "v1.5.6"
SOURCE_SHA = "1fadcbe9e69e1470f9093b6bcde08daf477d729c449e59a807f45c346622099b"
JSON_SHA = "aaf127c04cb31c406e5b04a63f1ae89369fccde6d8fa7cdda1ed4f32dfc5de63"


def download(url, path, sha=None):
    if not path.exists():
        print(f"Downloading {url}", flush=True)
        temporary = path.with_suffix(path.suffix + ".download")
        # curl uses the host's configured certificate store on macOS.
        subprocess.run(["curl", "--fail", "--location", "--silent", "--show-error", url, "-o", str(temporary)], check=True)
        temporary.replace(path)
    if sha and hashlib.sha256(path.read_bytes()).hexdigest() != sha:
        raise SystemExit(f"Checksum mismatch: {path}")


def run(args, **kwargs):
    print(" ".join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--skip-rust", action="store_true", help="reuse the previously built compiler archive")
    args = parser.parse_args()
    system = {"Darwin": "osx", "Linux": "linux"}.get(platform.system())
    arch = {"arm64": "arm64", "aarch64": "arm64", "x86_64": "amd64"}.get(platform.machine())
    if not system or not arch:
        raise SystemExit("Supported build hosts: Linux/macOS, ARM64/x86_64")
    vendor = ROOT / "vendor"
    build = ROOT / "build"
    vendor.mkdir(exist_ok=True)
    build.mkdir(exist_ok=True)
    source_tar = vendor / f"duckdb-{VERSION}.tar.gz"
    source = vendor / f"duckdb-{VERSION[1:]}"
    download(f"https://github.com/duckdb/duckdb/archive/refs/tags/{VERSION}.tar.gz", source_tar, SOURCE_SHA)
    if not source.exists():
        with tarfile.open(source_tar) as archive:
            archive.extractall(vendor, filter="data")
    download("https://raw.githubusercontent.com/nlohmann/json/v3.12.0/single_include/nlohmann/json.hpp", vendor / "json.hpp", JSON_SHA)
    cli_zip = vendor / f"duckdb-cli-{system}-{arch}.zip"
    download(f"https://github.com/duckdb/duckdb/releases/download/{VERSION}/duckdb_cli-{system}-{arch}.zip", cli_zip,
             "8e0f6825653f8d057922e6147db920bebf072cb41f4b041fd35521c18d7d126e" if (system, arch) == ("osx", "arm64") else None)
    cli = vendor / "cli" / "duckdb"
    if not cli.exists():
        with zipfile.ZipFile(cli_zip) as archive:
            archive.extractall(cli.parent)
    cli.chmod(0o755)
    version = subprocess.check_output([cli, "-csv", "-noheader", "-c", "SELECT version()"], text=True).strip()
    if version != VERSION:
        raise SystemExit(f"Expected {VERSION}, found {version}")
    target = Path(os.environ.get("CARGO_TARGET_DIR", str(ROOT.parent / "target"))).resolve()
    profile = "release" if args.release else "debug"
    archive = target / profile / "liborchid_duckdb_compiler.a"
    if not args.skip_rust:
        env = dict(os.environ, CARGO_TARGET_DIR=str(target))
        env.setdefault("CARGO_BUILD_JOBS", "4")
        run(["cargo", "build", "--locked", "--manifest-path", ROOT / "compiler/Cargo.toml"] + (["--release"] if args.release else []), env=env)
    obj = build / "orchid_extension.o"
    run([os.environ.get("CXX", "c++"), "-std=c++17", "-O2", "-fPIC", "-fvisibility=hidden", "-DDUCKDB_BUILD_LOADABLE_EXTENSION",
         "-I" + str(source / "src/include"), "-I" + str(vendor), "-c", ROOT / "src/orchid_extension.cpp", "-o", obj])
    binary = build / "orchid.unfooted"
    link = [os.environ.get("CXX", "c++")]
    if system == "osx":
        link += ["-dynamiclib", "-undefined", "dynamic_lookup", "-Wl,-exported_symbol,_orchid_duckdb_cpp_init"]
    else:
        link += ["-shared", "-Wl,--exclude-libs,ALL"]
    link += [obj, archive, "-o", binary]
    if system == "osx":
        link += ["-framework", "Security", "-framework", "CoreFoundation", "-liconv", "-lresolv"]
    else:
        link += ["-ldl", "-lpthread", "-lm"]
    run(link)
    duck_platform = subprocess.check_output([cli, "-csv", "-noheader", "-c", "PRAGMA platform"], text=True).strip()
    fields = ["4", duck_platform, VERSION, "0.1.0", "CPP", "", "", ""]
    metadata = b"".join(f.encode().ljust(32, b"\0") for f in reversed(fields)) + bytes(256)
    extension = build / "orchid.duckdb_extension"
    # Same custom section and metadata format as DuckDB's append_metadata.cmake.
    # Publish atomically: existing DuckDB processes may still map the old file.
    pending = extension.with_suffix('.pending')
    pending.write_bytes(binary.read_bytes() + b"\0\x93\x04\x10duckdb_signature\x80\x04" + metadata)
    pending.replace(extension)
    run([cli, "-unsigned", "-c", f"LOAD '{extension}'; SELECT 'Orchid extension loaded' AS status;"])
    print(f"\nBuilt {extension}\nCLI: {cli}")


if __name__ == "__main__":
    main()
