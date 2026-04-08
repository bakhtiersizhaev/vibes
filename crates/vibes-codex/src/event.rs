#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedCodexLine {
    Event(CodexEvent),
    Noise(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexEvent {
    SessionIdentified {
        session_id: String,
    },
    TurnStarted,
    AssistantMessage {
        text: String,
    },
    TextDelta {
        text: String,
    },
    CommandStarted {
        command: String,
    },
    CommandFinished {
        command: String,
        output: Option<String>,
        exit_code: Option<i64>,
    },
    ToolUse {
        command: Option<String>,
    },
    ToolResult {
        output: Option<String>,
    },
    FileChange {
        diff: String,
    },
    Reasoning,
    TurnCompleted {
        conclusion: RunConclusion,
    },
    Unknown {
        event_type: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunConclusion {
    Success,
    Failure,
    Unknown,
}
