//! Kafka CLI host. The authorizer is an application-owned executable, never a shell command.
use serde_json::{Value, json};
use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn run(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let mut options = std::collections::BTreeMap::new();
    let mut args = args.into_iter();
    while let Some(key) = args.next() {
        if ![
            "--schema",
            "--config",
            "--authorizer",
            "--subject",
            "--insert",
            "--insert-batch",
            "--query",
            "--subscribe",
            "--offsets",
            "--snapshot",
            "--parameters",
            "--max-records",
            "--wait-ms",
            "--batches",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("unknown stream option {key}").into());
        }
        if options
            .insert(key, args.next().ok_or("missing stream option value")?)
            .is_some()
        {
            return Err("duplicate stream option".into());
        }
    }
    let required = |key: &str| options.get(key).ok_or_else(|| format!("{key} required"));
    let read = |path: &str| -> Result<Value, Box<dyn Error>> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    };
    let schema = read(required("--schema")?)?;
    let mut config = read(required("--config")?)?;
    let authorizer = required("--authorizer")?;
    let subject = read(required("--subject")?)?;
    let actions: Vec<_> = ["--insert", "--insert-batch", "--query", "--subscribe"]
        .into_iter()
        .filter(|k| options.contains_key(*k))
        .collect();
    if actions.len() != 1 {
        return Err("choose exactly one of --insert, --insert-batch, --query, --subscribe".into());
    }
    if !config.is_object() {
        return Err("stream config must be an object".into());
    }
    config["op"] = json!("stream_open");
    config["schema"] = schema;
    let id = orchiddb::streaming::wire::command(config).await?["id"]
        .as_str()
        .ok_or("missing stream handle")?
        .to_owned();
    let outcome = async {
        let mut request = json!({"subject":subject,"parameters":{}});
        if let Some(p) = options.get("--parameters") {
            request["parameters"] = read(p)?;
        }
        match actions[0] {
            "--insert" => {
                request["action"] = json!("insert");
                request["change"] = read(required("--insert")?)?;
            }
            "--insert-batch" => {
                request["action"] = json!("insert_batch");
                request["changes"] = read(required("--insert-batch")?)?;
            }
            "--query" => {
                request["action"] = json!("query");
                request["text"] = json!(required("--query")?);
                if let Some(p) = options.get("--snapshot") {
                    request["action"] = json!("hybrid");
                    request["snapshot"] = read(p)?;
                }
            }
            _ => {
                request["action"] = json!("subscribe");
                request["text"] = json!(required("--subscribe")?);
                request["offsets"] = read(required("--offsets")?)?;
            }
        }
        let result = execute(&id, request, authorizer).await?;
        if actions[0] == "--subscribe" {
            let count: usize = options
                .get("--batches")
                .map(|s| s.parse())
                .transpose()?
                .unwrap_or(1);
            let max: usize = options
                .get("--max-records")
                .map(|s| s.parse())
                .transpose()?
                .unwrap_or(1);
            let wait: u64 = options
                .get("--wait-ms")
                .map(|s| s.parse())
                .transpose()?
                .unwrap_or(100);
            for _ in 0..count {
                let result = execute(
                    &id,
                    json!({"action":"next","max_records":max,"wait_ms":wait}),
                    authorizer,
                )
                .await?;
                println!("{result}");
                if result.is_null() {
                    break;
                }
            }
        } else {
            println!("{result}");
        }
        Ok::<_, Box<dyn Error>>(())
    }
    .await;
    let close = orchiddb::streaming::wire::command(json!({"op":"stream_close","id":id})).await;
    outcome?;
    close?;
    Ok(())
}
async fn execute(id: &str, mut request: Value, authorizer: &str) -> Result<Value, Box<dyn Error>> {
    request["op"] = json!("stream_start");
    request["id"] = json!(id);
    orchiddb::streaming::wire::command(request).await?;
    loop {
        let event = orchiddb::streaming::wire::command(json!({"op":"stream_poll","id":id})).await?;
        match event["state"].as_str() {
            Some("done") => return Ok(event["result"].clone()),
            Some("authorize") => {
                let mut child = tokio::process::Command::new(authorizer)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .kill_on_drop(true)
                    .spawn()?;
                let body = serde_json::to_vec(
                    &json!({"subject":event["subject"],"checks":event["checks"]}),
                )?;
                let decisions: Vec<bool> =
                    tokio::time::timeout(std::time::Duration::from_secs(30), async {
                        let mut stdin = child.stdin.take().ok_or("authorizer stdin missing")?;
                        stdin.write_all(&body).await?;
                        drop(stdin);
                        let stdout = child.stdout.take().ok_or("authorizer stdout missing")?;
                        let mut output = Vec::new();
                        stdout.take(65_537).read_to_end(&mut output).await?;
                        if output.len() > 65_536 {
                            return Err::<Vec<bool>, Box<dyn Error>>(
                                "authorizer response too large".into(),
                            );
                        }
                        if !child.wait().await?.success() {
                            return Err("authorizer process failed".into());
                        }
                        Ok(serde_json::from_slice(&output)?)
                    })
                    .await
                    .map_err(|_| "authorizer process timed out")??;
                if decisions.len() != event["checks"].as_array().ok_or("invalid checks")?.len() {
                    return Err("authorizer must return one boolean per check".into());
                }
                orchiddb::streaming::wire::command(json!({"op":"stream_authorize","id":id,"nonce":event["nonce"],"decisions":decisions})).await?;
            }
            _ => {}
        }
    }
}
