use arrow::{array::StringArray, ipc::reader::StreamReader};
use std::process::Command;
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_orchiddb"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn native_arrow_output_and_application_setup() {
    let output = cli(&[
        "query",
        "examples/people.json",
        "--init",
        "examples/setup.sql",
        "--no-iceberg",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reader = StreamReader::try_new(std::io::Cursor::new(output.stdout), None).unwrap();
    let mut names = Vec::new();
    for batch in reader {
        let batch = batch.unwrap();
        let col = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        names.extend(col.iter().flatten().map(str::to_owned));
    }
    assert_eq!(names, ["Ada", "Grace"]);
}
#[test]
fn compile_does_not_need_a_database_or_extension() {
    let output = cli(&["compile", "examples/people.json"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("SELECT"));
}
#[test]
fn invalid_arguments_fail() {
    assert!(
        !cli(&["query", "examples/people.json", "--format", "bad"])
            .status
            .success()
    );
}
#[test]
fn default_iceberg_load_is_real() {
    // CI and release smoke tests deliberately exercise signed extension installation.
    let setup = std::env::temp_dir().join(format!("orchiddb-iceberg-{}.sql", std::process::id()));
    std::fs::write(&setup, "CREATE VIEW people AS SELECT 1::BIGINT id, CASE WHEN loaded THEN 'Iceberg loaded' ELSE error('Iceberg missing') END AS name FROM duckdb_extensions() WHERE extension_name='iceberg' AND (SELECT count(*) FROM iceberg_scan('examples/empty-iceberg.metadata.json')) = 0;").unwrap();
    let output = cli(&[
        "query",
        "examples/people.json",
        "--init",
        setup.to_str().unwrap(),
        "--format",
        "table",
    ]);
    std::fs::remove_file(setup).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Iceberg loaded"));
}

#[test]
fn one_time_statistics_can_be_saved_and_used_without_a_database() {
    let path =
        std::env::temp_dir().join(format!("orchiddb-statistics-{}.json", std::process::id()));
    let generated = cli(&[
        "statistics",
        "examples/people.json",
        "--init",
        "examples/setup.sql",
        "--no-iceberg",
        "--output",
        path.to_str().unwrap(),
    ]);
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let snapshot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(snapshot.is_object());
    assert_eq!(snapshot["sources"]["people"]["sample_rows"], 2);
    assert!(
        snapshot["report"]["skipped"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    let compiled = cli(&[
        "compile",
        "examples/people.json",
        "--statistics",
        path.to_str().unwrap(),
        "--explain-json",
    ]);
    std::fs::remove_file(path).unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&compiled.stdout).unwrap();
    assert!(plan.get("statistics_usage").is_some());
    assert!(plan.get("plan_estimates").is_some());
}
