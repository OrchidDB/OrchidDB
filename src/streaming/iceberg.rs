use super::*;
/// A caller-owned DuckDB session with Iceberg already configured. `snapshot_sql`
/// must read one pinned Iceberg snapshot and return exactly one VARCHAR column
/// containing a serialized Snapshot. Store the checkpoint and versioned records
/// in one atomically committed manifest; never read offsets from a newer commit.
/// The SQL is trusted host configuration, never query text supplied by a user.
pub struct DuckDbSnapshot<'a> {
    pub connection: &'a duckdb::Connection,
    pub snapshot_sql: String,
}
#[async_trait(?Send)]
impl SnapshotSource for DuckDbSnapshot<'_> {
    async fn load(&mut self) -> Result<Snapshot, String> {
        let mut statement = self
            .connection
            .prepare(&self.snapshot_sql)
            .map_err(|e| e.to_string())?;
        let mut rows = statement.query([]).map_err(|e| e.to_string())?;
        let value: String = rows
            .next()
            .map_err(|e| e.to_string())?
            .ok_or("Iceberg checkpoint missing")?
            .get(0)
            .map_err(|e| e.to_string())?;
        if rows.next().map_err(|e| e.to_string())?.is_some() {
            return Err("Iceberg checkpoint query must return exactly one manifest".into());
        }
        serde_json::from_str(&value).map_err(|e| e.to_string())
    }
}
