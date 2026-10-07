#![cfg(all(feature = "quickwit", feature = "elasticsearch"))]
use serde_json::{Value, json};

#[test]
fn live_remote_queries_use_explicit_engine_configuration() {
    let Ok(path) = std::env::var("ORCHIDDB_REMOTE_FIXTURE") else {
        return;
    };
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let root = std::env::temp_dir().join(format!("orchiddb-cli-remote-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let request = root.join("request.json");
        let init = root.join("setup.sql");
        let engines = root.join("engines.json");
        let mut schema = case["request"].clone();
        for key in ["query", "language", "version", "dialect", "parameters"] {
            schema.as_object_mut().unwrap().remove(key);
        }
        std::fs::write(&request, schema.to_string()).unwrap();
        std::fs::write(
            &init,
            case["setup_sql"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(";\n"),
        )
        .unwrap();
        std::fs::write(
            &engines,
            json!({"text":{"endpoint":case["endpoint"],"page_size":2,"batch_size":2}}).to_string(),
        )
        .unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_orchiddb"))
            .args([
                "query",
                case["request"]["query"].as_str().unwrap(),
                "--schema",
                request.to_str().unwrap(),
                "--init",
                init.to_str().unwrap(),
                "--engines",
                engines.to_str().unwrap(),
                "--no-iceberg",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}: {}",
            case["name"],
            String::from_utf8_lossy(&output.stderr)
        );
        let reader =
            arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(output.stdout), None)
                .unwrap();
        let mut rows = vec![];
        for batch in reader {
            let batch = batch.unwrap();
            for row in 0..batch.num_rows() {
                rows.push(
                    batch
                        .columns()
                        .iter()
                        .map(|a| arrow::util::display::array_value_to_string(a, row).unwrap())
                        .collect::<Vec<_>>(),
                );
            }
        }
        let expected: Vec<Vec<_>> = case["expected_rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                row.as_array()
                    .unwrap()
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string())
                    })
                    .collect()
            })
            .collect();
        assert_eq!(rows, expected, "{}", case["name"]);
    }
    std::fs::remove_dir_all(root).unwrap();
}
