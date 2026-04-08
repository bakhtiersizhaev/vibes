use vibes_core::short_session_id;

const TELEGRAM_TOPIC_TITLE_LIMIT: usize = 64;

pub fn build_topic_title(label: Option<&str>, session_id: &str) -> String {
    let normalized_label = label.map(str::trim).filter(|value| !value.is_empty());
    let suffix = normalized_label.unwrap_or_else(|| short_session_id(session_id));
    let raw = format!("codex-{suffix}");
    trim_topic_title(&raw)
}

fn trim_topic_title(raw: &str) -> String {
    let mut trimmed = raw.trim().replace(char::is_whitespace, "-");
    if trimmed.len() > TELEGRAM_TOPIC_TITLE_LIMIT {
        trimmed.truncate(TELEGRAM_TOPIC_TITLE_LIMIT);
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use crate::topic_title::build_topic_title;

    #[test]
    fn prefers_human_label_when_present() {
        assert_eq!(
            build_topic_title(Some("worldmonitor"), "019d6361-f755-7992-b08a"),
            "codex-worldmonitor"
        );
    }

    #[test]
    fn falls_back_to_short_session_id() {
        assert_eq!(
            build_topic_title(None, "019d6361-f755-7992-b08a"),
            "codex-019d6361"
        );
    }

    #[test]
    fn trims_oversized_topic_titles() {
        let title = build_topic_title(
            Some("this label is intentionally much longer than the telegram topic title limit"),
            "019d6361-f755-7992-b08a",
        );
        assert!(title.len() <= 64);
        assert!(title.starts_with("codex-"));
    }
}
