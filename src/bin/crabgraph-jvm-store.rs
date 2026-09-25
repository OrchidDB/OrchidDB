//! Persistent JSON-lines storage process used by the JVM CrabGraph provider.
use orchiddb::jvm_bridge::Store;
use std::io::{self, BufRead, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut store = match args.next().as_deref() {
        None => Store::new(),
        Some("--path") => Store::open(args.next().ok_or("--path requires a file")?)?,
        Some(_) => return Err("usage: crabgraph-jvm-store [--path SNAPSHOT]".into()),
    };
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let response = match serde_json::from_str::<serde_json::Value>(&line?) {
            Ok(request) => store.request(&request),
            Err(e) => store.protocol_error(format!("invalid JSON request: {e}")),
        };
        serde_json::to_writer(&mut stdout, &response)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        if store.is_closed() {
            break;
        }
    }
    Ok(())
}
