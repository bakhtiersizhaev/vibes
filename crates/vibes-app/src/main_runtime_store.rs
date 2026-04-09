use anyhow::Context;
use vibes_store::SqliteBindingStore;

pub(crate) fn open_sqlite_store(db_path: &str) -> anyhow::Result<SqliteBindingStore> {
    SqliteBindingStore::open(db_path)
        .with_context(|| format!("failed to open sqlite store at {db_path}"))
}
