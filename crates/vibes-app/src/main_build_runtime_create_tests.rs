use teloxide::Bot;

use crate::main_runtime_components::build_runtime_components;
use crate::main_test_support::unique_db_path;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_runtime_components_creates_sqlite_store_at_path() {
        let db_path = unique_db_path("vibes-build-runtime-create-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let _ = build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();

        assert!(db_path.exists());
        std::fs::remove_file(db_path).unwrap();
    }

}
