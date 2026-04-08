use serde_json::Value;

use crate::event::{CodexEvent, ParsedCodexLine, RunConclusion};

pub fn parse_codex_line(line: &str) -> ParsedCodexLine {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ParsedCodexLine::Noise(String::new());
    }

    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return ParsedCodexLine::Noise(trimmed.to_owned());
    };

    let Some(object) = value.as_object() else {
        return ParsedCodexLine::Noise(trimmed.to_owned());
    };

    let event_type =
        extract_string_by_keys(&value, &["type", "event", "kind", "name"]).unwrap_or_default();

    if matches!(
        event_type.as_str(),
        "thread.started" | "thread_started" | "thread.start"
    ) && let Some(session_id) = extract_session_id(&value)
    {
        return ParsedCodexLine::Event(CodexEvent::SessionIdentified { session_id });
    }

    if matches!(event_type.as_str(), "turn.started" | "turn_started") {
        return ParsedCodexLine::Event(CodexEvent::TurnStarted);
    }

    if matches!(event_type.as_str(), "turn.completed" | "turn_completed") {
        return ParsedCodexLine::Event(CodexEvent::TurnCompleted {
            conclusion: extract_conclusion(&value),
        });
    }

    if event_type.starts_with("item.")
        && let Some(item) = extract_item(&value)
        && let Some(event) = parse_item_event(&event_type, item)
    {
        return ParsedCodexLine::Event(event);
    }

    if event_type == "text"
        && let Some(text) = extract_textish(&value)
    {
        return ParsedCodexLine::Event(CodexEvent::TextDelta { text });
    }

    if event_type == "assistant_message"
        && let Some(text) = extract_textish(&value)
    {
        return ParsedCodexLine::Event(CodexEvent::AssistantMessage { text });
    }

    if event_type == "tool_use" {
        return ParsedCodexLine::Event(CodexEvent::ToolUse {
            command: extract_command(&value),
        });
    }

    if event_type == "tool_result" {
        return ParsedCodexLine::Event(CodexEvent::ToolResult {
            output: extract_output(&value),
        });
    }

    if event_type == "file_change"
        && let Some(diff) = extract_string_by_keys(&value, &["diff", "patch", "unified_diff"])
    {
        return ParsedCodexLine::Event(CodexEvent::FileChange { diff });
    }

    if let Some(session_id) = extract_session_id(&value) {
        return ParsedCodexLine::Event(CodexEvent::SessionIdentified { session_id });
    }

    ParsedCodexLine::Event(CodexEvent::Unknown {
        event_type: if event_type.is_empty() {
            object.keys().cloned().collect::<Vec<_>>().join(",")
        } else {
            event_type
        },
    })
}

fn parse_item_event(event_type: &str, item: &Value) -> Option<CodexEvent> {
    let item_type = extract_string_by_keys(item, &["type"]).unwrap_or_default();

    if item_type == "reasoning" {
        return Some(CodexEvent::Reasoning);
    }

    if item_type == "command_execution" {
        let command = extract_command(item)?;
        let status = extract_string_by_keys(item, &["status"]).unwrap_or_default();
        let is_start = event_type.ends_with("started") || status == "in_progress";
        if is_start {
            return Some(CodexEvent::CommandStarted { command });
        }
        return Some(CodexEvent::CommandFinished {
            command,
            output: extract_string_by_keys(item, &["aggregated_output", "stdout", "output"]),
            exit_code: extract_i64_by_keys(item, &["exit_code"]),
        });
    }

    if item_type == "assistant_message"
        && let Some(text) = extract_textish(item)
    {
        return Some(CodexEvent::AssistantMessage { text });
    }

    extract_textish(item).map(|text| CodexEvent::TextDelta { text })
}

fn extract_item(value: &Value) -> Option<&Value> {
    value
        .get("item")
        .or_else(|| value.get("data").and_then(|data| data.get("item")))
}

fn extract_session_id(value: &Value) -> Option<String> {
    [
        value.pointer("/session_id"),
        value.pointer("/thread_id"),
        value.pointer("/thread/id"),
        value.pointer("/session/id"),
        value.pointer("/data/session_id"),
        value.pointer("/data/thread_id"),
        value.pointer("/data/thread/id"),
        value.pointer("/data/session/id"),
    ]
    .into_iter()
    .flatten()
    .find_map(|candidate| candidate.as_str().map(str::trim).filter(|s| !s.is_empty()))
    .map(str::to_owned)
}

fn extract_conclusion(value: &Value) -> RunConclusion {
    match value
        .pointer("/success")
        .or_else(|| value.pointer("/data/success"))
        .and_then(Value::as_bool)
    {
        Some(true) => RunConclusion::Success,
        Some(false) => RunConclusion::Failure,
        None => RunConclusion::Unknown,
    }
}

fn extract_textish(value: &Value) -> Option<String> {
    extract_string_by_keys(value, &["delta", "text", "content"]).or_else(|| {
        value
            .get("data")
            .and_then(|nested| extract_string_by_keys(nested, &["delta", "text", "content"]))
    })
}

fn extract_command(value: &Value) -> Option<String> {
    extract_string_by_keys(value, &["command", "cmd"]).or_else(|| {
        value
            .get("input")
            .and_then(|input| extract_string_by_keys(input, &["command", "cmd"]))
    })
}

fn extract_output(value: &Value) -> Option<String> {
    extract_string_by_keys(value, &["output", "stdout", "result", "text"]).or_else(|| {
        value.get("data").and_then(|nested| {
            extract_string_by_keys(nested, &["output", "stdout", "result", "text"])
        })
    })
}

fn extract_string_by_keys(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_owned)
}

fn extract_i64_by_keys(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| match value.get(*key) {
        Some(Value::Number(number)) => number.as_i64(),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use crate::event::{CodexEvent, ParsedCodexLine, RunConclusion};
    use crate::parser::parse_codex_line;

    #[test]
    fn parses_thread_started_with_nested_id() {
        let parsed = parse_codex_line(
            r#"{"type":"thread.started","thread":{"id":"019d6361-f755-7992-b08a"}}"#,
        );
        assert_eq!(
            parsed,
            ParsedCodexLine::Event(CodexEvent::SessionIdentified {
                session_id: "019d6361-f755-7992-b08a".to_owned(),
            })
        );
    }

    #[test]
    fn parses_command_completion_events() {
        let parsed = parse_codex_line(
            r#"{"type":"item.completed","item":{"type":"command_execution","command":"ls","status":"completed","aggregated_output":"file1\nfile2","exit_code":0}}"#,
        );

        assert_eq!(
            parsed,
            ParsedCodexLine::Event(CodexEvent::CommandFinished {
                command: "ls".to_owned(),
                output: Some("file1\nfile2".to_owned()),
                exit_code: Some(0),
            })
        );
    }

    #[test]
    fn parses_tool_use_and_noise() {
        assert_eq!(
            parse_codex_line(r#"{"type":"tool_use","input":{"command":"echo hi"}}"#),
            ParsedCodexLine::Event(CodexEvent::ToolUse {
                command: Some("echo hi".to_owned()),
            })
        );
        assert_eq!(
            parse_codex_line("not-json"),
            ParsedCodexLine::Noise("not-json".to_owned())
        );
    }

    #[test]
    fn parses_turn_completion_success_flag() {
        let parsed = parse_codex_line(r#"{"type":"turn.completed","success":true}"#);
        assert_eq!(
            parsed,
            ParsedCodexLine::Event(CodexEvent::TurnCompleted {
                conclusion: RunConclusion::Success,
            })
        );
    }
}
