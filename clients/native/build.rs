use std::{env, path::PathBuf, process::Command};
fn main() {
    // Cargo supplies the active checkout at build-script execution time.
    // Do not bake a previous checkout into a shared target-cache executable.
    let core = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("../..");
    let git = |args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(&core)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let head = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = git(&["status", "--porcelain", "--untracked-files=normal"])
        .map(|s| !s.is_empty())
        .unwrap_or(true);
    if env::var("ORCHIDDB_RELEASE_BUILD").as_deref() == Ok("1") {
        assert_ne!(head, "unknown", "release requires an identified source checkout");
        assert!(!dirty, "release requires a clean core checkout");
    }
    let revision = if dirty { format!("{head}-dirty") } else { head };
    println!("cargo:rustc-env=ORCHIDDB_BUILT_CORE_REVISION={revision}");
    println!("cargo:rerun-if-env-changed=ORCHIDDB_RELEASE_BUILD");

    // Observe shared source and repository revision changes.
    println!("cargo:rerun-if-changed={}", core.join("src").display());
    println!(
        "cargo:rerun-if-changed={}",
        core.join(".git/HEAD").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        core.join(".git/refs/heads").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        core.join(".git/index").display()
    );
}
