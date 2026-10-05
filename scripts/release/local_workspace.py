"""Assemble release manifests around immutable source without changing the tags."""
import json
from pathlib import Path
import re
import shutil
import tomllib

PACKAGES = ['orchiddb-compiler-native', 'orchiddb-java-native', 'orchiddb-cli']
MEMBERS = ['orchiddb-native', 'orchiddb-java/native', 'orchiddb-rust', 'orchiddb-cli']
FEATURES = ','.join(f'{name}/{feature}' for name in PACKAGES for feature in ['quickwit', 'elasticsearch'])
LOCK = Path(__file__).with_name('workspace.Cargo.lock')


def write_changed(path, text):
    if not path.exists() or path.read_text() != text:
        path.write_text(text)


def prepare(sources, destination, lock=LOCK):
    destination.mkdir(parents=True, exist_ok=True)
    # The C ABI provenance build script resolves this sibling checkout.
    core = destination / 'orchiddb'
    if not core.exists():
        core.symlink_to(sources / 'orchiddb', target_is_directory=True)
    for member in MEMBERS:
        source = sources / member
        target = destination / member
        target.mkdir(parents=True, exist_ok=True)
        for path in source.iterdir():
            if path.name in ['Cargo.toml', 'Cargo.lock', '.git', 'target']:
                continue
            link = target / path.name
            if not link.exists():
                link.symlink_to(path, target_is_directory=path.is_dir())
        manifest = (source / 'Cargo.toml').read_text()
        # All three outputs resolve the exact same core package identity.
        manifest = re.sub(r'^orchiddb = \{[^\n]+',
                          'orchiddb = { path = ' + json.dumps(str(sources / 'orchiddb')) + ', default-features = false }', manifest, flags=re.M)
        manifest = re.sub(r'^orchiddb-client = \{[^\n]+',
                          'orchiddb-client = { path = "../orchiddb-rust", default-features = false }', manifest, flags=re.M)
        manifest = re.sub(r'\n\[profile\.[^\]]+\]\n.*?(?=\n\[|\Z)', '\n', manifest, flags=re.S)
        if member == 'orchiddb-cli':
            if (source / 'build.rs').exists() or 'build' in tomllib.loads(manifest)['package']:
                raise ValueError('Review CLI link setup: source now has a build script')
            manifest = manifest.replace('[package]', '[package]\nbuild = "release-link.rs"', 1)
            # Scope C++ linkage to the CLI, avoiding global RUSTFLAGS that
            # invalidate every dependency for the other output libraries.
            write_changed(target / 'release-link.rs', '''fn main() {
    println!("cargo:rerun-if-env-changed=ORCHIDDB_CLI_CXX_DIR");
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "linux" {
        let path = std::env::var("ORCHIDDB_CLI_CXX_DIR").expect("Linux C++ sysroot");
        println!("cargo:rustc-link-search=native={path}");
        println!("cargo:rustc-link-lib=static=stdc++");
    } else {
        println!("cargo:rustc-link-lib=c++");
    }
}
''')
        write_changed(target / 'Cargo.toml', manifest)
    write_changed(destination / 'Cargo.toml', '[workspace]\nresolver = "2"\nmembers = ' + json.dumps(MEMBERS) +
                  '\nexclude = ["orchiddb"]\n\n[profile.release]\nstrip = true\nlto = "thin"\ncodegen-units = 1\n')
    if lock is not None:
        write_changed(destination / 'Cargo.lock', lock.read_text())
    return destination / 'Cargo.toml'


def selection():
    return [item for name in PACKAGES for item in ['--package', name]] + ['--no-default-features', '--features', FEATURES]
