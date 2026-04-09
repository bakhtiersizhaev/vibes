use teloxide::Bot;

use crate::main_runtime_builder::build_runtime_components;
use crate::main_test_support::unique_db_path;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_runtime_components_returns_error_for_invalid_db_path() {
        let db_path = unique_db_path("vibes-build-runtime-error-tests")
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
