use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use teloxide::Bot;

use crate::main_startup::build_runtime_components;

static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_db_path() -> PathBuf {
    let pid = std::process::id();
    let n = TEST_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("vibes-build-runtime-{pid}-{n}.sqlite3"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_runtime_components_creates_sqlite_store_at_path() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let _ = build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();

        assert!(db_path.exists());
        std::fs::remove_file(db_path).unwrap();
    }

    #[test]
    fn build_runtime_components_reopens_existing_sqlite_store() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let _ = build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let _ = build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();

        assert!(db_path.exists());
        std::fs::remove_file(db_path).unwrap();
    }

    #[test]
    fn build_runtime_components_returns_error_for_invalid_db_path() {
        let db_path = unique_db_path()
            .with_extension("")
            .join("nested")
            .join("vibes.sqlite3");

        let bot = Bot::new("123456:TESTTOKEN");
        let path_string = db_path.to_str().unwrap().to_owned();
        let rendered = match build_runtime_components(&bot, &path_string) {
            Ok(_) => panic!("expected sqlite open failure"),
            Err(err) => err.to_string(),
        };

        assert!(rendered.contains("failed to open sqlite store"));
        assert!(rendered.contains(&path_string));
    }


}
