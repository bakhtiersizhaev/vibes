use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};


#[cfg(test)]
mod tests {
    use super::*;

    static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_db_path() -> PathBuf {
        let pid = std::process::id();
        let n = TEST_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("vibes-build-runtime-{pid}-{n}.sqlite3"))
    }


}
