use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCommandInput {
    Command(SessionCommand),
    Prompt(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCommand {
    New { label: Option<String> },
    Resume { target: String },
    Status,
    Sessions,
    Help,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CommandParseError {
    #[error("missing argument for /resume")]
    MissingResumeTarget,
    #[error("unknown command: {0}")]
    UnknownCommand(String),
    #[error("command is addressed to a different bot: {0}")]
    ForeignBotCommand(String),
}

pub fn parse_user_input(
    input: &str,
    bot_username: Option<&str>,
) -> Result<SessionCommandInput, CommandParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(SessionCommandInput::Prompt(String::new()));
    }

    if !trimmed.starts_with('/') {
        return Ok(SessionCommandInput::Prompt(trimmed.to_owned()));
    }

    let without_slash = &trimmed[1..];
    let (command_token, raw_tail) = match without_slash.split_once(char::is_whitespace) {
        Some((command, tail)) => (command, tail.trim()),
        None => (without_slash, ""),
    };
    let (command_name, mention) = split_command_mention(command_token);

    if let Some(mention) = mention {
        let expected = bot_username
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if !expected.is_empty() && mention.to_ascii_lowercase() != expected {
            return Err(CommandParseError::ForeignBotCommand(mention.to_owned()));
        }
    }

    let command = match command_name {
        "new" => SessionCommand::New {
            label: normalize_optional_tail(raw_tail),
        },
        "resume" => SessionCommand::Resume {
            target: normalize_required_tail(raw_tail)?,
        },
        "status" => SessionCommand::Status,
        "sessions" => SessionCommand::Sessions,
        "help" | "start" => SessionCommand::Help,
        other => return Err(CommandParseError::UnknownCommand(other.to_owned())),
    };

    Ok(SessionCommandInput::Command(command))
}

fn split_command_mention(command_token: &str) -> (&str, Option<&str>) {
    match command_token.split_once('@') {
        Some((command, mention)) if !mention.is_empty() => (command, Some(mention)),
        _ => (command_token, None),
    }
}

fn normalize_optional_tail(tail: &str) -> Option<String> {
    let normalized = tail.trim();
    (!normalized.is_empty()).then(|| normalized.to_owned())
}

fn normalize_required_tail(tail: &str) -> Result<String, CommandParseError> {
    normalize_optional_tail(tail).ok_or(CommandParseError::MissingResumeTarget)
}

#[cfg(test)]
mod tests {
    use super::{CommandParseError, SessionCommand, SessionCommandInput, parse_user_input};

    #[test]
    fn parses_new_without_label() {
        let parsed = parse_user_input("/new", Some("vibes_bot")).unwrap();
        assert_eq!(
            parsed,
            SessionCommandInput::Command(SessionCommand::New { label: None })
        );
    }

    #[test]
    fn parses_new_with_label_and_bot_mention() {
        let parsed = parse_user_input("/new@vibes_bot rust rewrite", Some("vibes_bot")).unwrap();
        assert_eq!(
            parsed,
            SessionCommandInput::Command(SessionCommand::New {
                label: Some("rust rewrite".to_owned()),
            })
        );
    }

    #[test]
    fn parses_resume_target() {
        let parsed = parse_user_input("/resume 019d6361-f755-7992-b08a", None).unwrap();
        assert_eq!(
            parsed,
            SessionCommandInput::Command(SessionCommand::Resume {
                target: "019d6361-f755-7992-b08a".to_owned(),
            })
        );
    }

    #[test]
    fn rejects_resume_without_target() {
        let error = parse_user_input("/resume   ", None).unwrap_err();
        assert_eq!(error, CommandParseError::MissingResumeTarget);
    }

    #[test]
    fn rejects_foreign_bot_command() {
        let error = parse_user_input("/new@other_bot x", Some("vibes_bot")).unwrap_err();
        assert_eq!(
            error,
            CommandParseError::ForeignBotCommand("other_bot".to_owned())
        );
    }

    #[test]
    fn treats_plain_text_as_prompt() {
        let parsed = parse_user_input("continue work on parser", Some("vibes_bot")).unwrap();
        assert_eq!(
            parsed,
            SessionCommandInput::Prompt("continue work on parser".to_owned())
        );
    }
}
