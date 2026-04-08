use crate::event::{CodexEvent, RunConclusion};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CodexTranscript {
    session_id: Option<String>,
    lines: Vec<String>,
    last_command: Option<String>,
    conclusion: Option<RunConclusion>,
}

impl CodexTranscript {
    pub fn apply(&mut self, event: CodexEvent) {
        match event {
            CodexEvent::SessionIdentified { session_id } => self.session_id = Some(session_id),
            CodexEvent::AssistantMessage { text } | CodexEvent::TextDelta { text } => {
                self.push_line(text);
            }
            CodexEvent::CommandStarted { command } => {
                if self.last_command.as_deref() != Some(command.as_str()) {
                    self.push_line(format!("$ {command}"));
                    self.last_command = Some(command);
                }
            }
            CodexEvent::CommandFinished {
                command,
                output,
                exit_code,
            } => {
                if self.last_command.as_deref() != Some(command.as_str()) {
                    self.push_line(format!("$ {command}"));
                }
                if let Some(output) = output.filter(|value| !value.trim().is_empty()) {
                    self.push_line(output);
                }
                if let Some(exit_code) = exit_code {
                    self.push_line(format!("(exit_code: {exit_code})"));
                }
                self.last_command = Some(command);
            }
            CodexEvent::ToolUse { command } => {
                self.push_line(format!(
                    "[tool_use]\n{}",
                    command.unwrap_or_else(|| "<structured tool call>".to_owned())
                ));
            }
            CodexEvent::ToolResult { output } => {
                self.push_line(format!(
                    "[tool_result]\n{}",
                    output.unwrap_or_else(|| "<empty>".to_owned())
                ));
            }
            CodexEvent::FileChange { diff } => self.push_line(format!("[file_change]\n{diff}")),
            CodexEvent::TurnCompleted { conclusion } => self.conclusion = Some(conclusion),
            CodexEvent::TurnStarted | CodexEvent::Reasoning | CodexEvent::Unknown { .. } => {}
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn rendered(&self) -> String {
        self.lines.join("\n")
    }

    pub fn conclusion(&self) -> Option<RunConclusion> {
        self.conclusion
    }

    fn push_line(&mut self, line: String) {
        if !line.is_empty() {
            self.lines.push(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::event::{CodexEvent, RunConclusion};
    use crate::transcript::CodexTranscript;

    #[test]
    fn transcript_ignores_reasoning_and_keeps_session_id() {
        let mut transcript = CodexTranscript::default();
        transcript.apply(CodexEvent::SessionIdentified {
            session_id: "019d6361-f755-7992-b08a".to_owned(),
        });
        transcript.apply(CodexEvent::Reasoning);
        transcript.apply(CodexEvent::TextDelta {
            text: "hello".to_owned(),
        });
        transcript.apply(CodexEvent::TurnCompleted {
            conclusion: RunConclusion::Success,
        });

        assert_eq!(transcript.session_id(), Some("019d6361-f755-7992-b08a"));
        assert_eq!(transcript.rendered(), "hello");
        assert_eq!(transcript.conclusion(), Some(RunConclusion::Success));
    }

    #[test]
    fn transcript_deduplicates_running_command_banner() {
        let mut transcript = CodexTranscript::default();
        transcript.apply(CodexEvent::CommandStarted {
            command: "ls".to_owned(),
        });
        transcript.apply(CodexEvent::CommandFinished {
            command: "ls".to_owned(),
            output: Some("file1".to_owned()),
            exit_code: Some(0),
        });

        assert_eq!(transcript.rendered(), "$ ls\nfile1\n(exit_code: 0)");
    }
}
