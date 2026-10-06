//! Database dependency discovery for the shared effect ordering.
use crate::ir::rel::sql::{mutation::MappedMutation, DuckDbExecutor};
pub(super) fn order(effects: &mut [MappedMutation], executor: &mut DuckDbExecutor) -> Result<(), String> {
    let conn = executor.connection().map_err(|e| e.to_string())?;
    let mut statement = conn.prepare("SELECT table_name, referenced_table FROM duckdb_constraints() WHERE constraint_type='FOREIGN KEY'").map_err(|e| e.to_string())?;
    let dependencies = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    crate::language::sparql::order_mutations(effects, &dependencies)
}
