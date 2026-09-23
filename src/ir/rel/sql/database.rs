//! Share a DuckDB database instance across file-backed engine sessions.
//!
//! The pinned DuckDB binding gives independent `Connection::open` calls
//! independent caches. Connections must clone the same instance to share
//! committed state and detect conflicting writes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use duckdb::Connection;

pub(crate) type SharedDatabase = Arc<Mutex<Connection>>;

pub(crate) fn open_shared(path: &Path) -> Result<(Connection, Option<SharedDatabase>), String> {
    if path == Path::new(":memory:") {
        return Connection::open_in_memory()
            .map(|connection| (connection, None))
            .map_err(|e| e.to_string());
    }
    let key = if path.exists() {
        std::fs::canonicalize(path).map_err(|e| e.to_string())?
    } else {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path)
        };
        let parent = absolute.parent().ok_or("database path has no parent")?;
        std::fs::canonicalize(parent)
            .map_err(|e| e.to_string())?
            .join(
                absolute
                    .file_name()
                    .ok_or("database path has no filename")?,
            )
    };
    type Databases = BTreeMap<PathBuf, Weak<Mutex<Connection>>>;
    static DATABASES: OnceLock<Mutex<Databases>> = OnceLock::new();
    let mut databases = DATABASES
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "database registry lock poisoned")?;
    databases.retain(|_, database| database.strong_count() > 0);
    let database = match databases.get(&key).and_then(Weak::upgrade) {
        Some(database) => database,
        None => {
            let database = Arc::new(Mutex::new(
                Connection::open(&key).map_err(|e| e.to_string())?,
            ));
            databases.insert(key, Arc::downgrade(&database));
            database
        }
    };
    drop(databases);
    let connection = database
        .lock()
        .map_err(|_| "database connection lock poisoned")?
        .try_clone()
        .map_err(|e| e.to_string())?;
    Ok((connection, Some(database)))
}
