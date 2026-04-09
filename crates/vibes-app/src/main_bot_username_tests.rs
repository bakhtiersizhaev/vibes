#[cfg(test)]
mod tests {
    use crate::main_support::bot_username;

    #[test]
    fn bot_username_returns_some_when_present() {
        let user: teloxide::types::User = serde_json::from_str(
            r#"{"id":1,"is_bot":true,"first_name":"Vibes","username":"vibes_bot"}"#,
        )
        .unwrap();

        assert_eq!(bot_username(&user).as_deref(), Some("vibes_bot"));
    }

    #[test]
    fn bot_username_returns_none_when_missing() {
        let user: teloxide::types::User =
            serde_json::from_str(r#"{"id":1,"is_bot":true,"first_name":"Vibes"}"#).unwrap();

        assert_eq!(bot_username(&user), None);
    }

    #[test]
    fn bot_username_returns_none_for_empty_string() {
        let user: teloxide::types::User =
            serde_json::from_str(r#"{"id":1,"is_bot":true,"first_name":"Vibes","username":""}"#)
                .unwrap();

        assert_eq!(bot_username(&user), None);
    }

}
