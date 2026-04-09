use teloxide::Bot;

use crate::main_startup::build_runtime_components;
use crate::main_test_support::unique_db_path;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_runtime_components_reopens_existing_sqlite_store() {
        let db_path = unique_db_path("vibes-build-runtime-reopen-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let _ = build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let _ = build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();

        assert!(db_path.exists());
        std::fs::remove_file(db_path).unwrap();
    }

}
