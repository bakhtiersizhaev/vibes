use std::path::Path;
use std::process::Command;

use thiserror::Error;
use vibes_core::SessionHandle;

use crate::{CodexEvent, CodexTranscript, ParsedCodexLine, parse_codex_line};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexRunRequest {
    pub prompt: String,
    pub resume_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexRunResult {
    pub session_id: Option<String>,
    pub transcript: CodexTranscript,
    pub events: Vec<CodexEvent>,
}

#[derive(Debug, Clone)]
pub struct CodexExecRunner {
    binary: String,
}

#[derive(Debug, Error)]
pub enum CodexRunError {
    #[error("failed to spawn codex: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("codex exited with non-zero code {code:?}: {stderr}")]
    NonZeroExit { code: Option<i32>, stderr: String },
    #[error("codex output did not include session id")]
    MissingSessionId,
}

impl Default for CodexExecRunner {
    fn default() -> Self {
        Self {
            binary: "codex".to_owned(),
        }
    }
}

impl CodexExecRunner {
    pub fn with_binary(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    pub fn run(
        &self,
        request: &CodexRunRequest,
        cwd: &Path,
    ) -> Result<CodexRunResult, CodexRunError> {
        let output = Command::new(&self.binary)
            .args(build_exec_args(request))
            .current_dir(cwd)
            .output()?;

        if !output.status.success() {
            return Err(CodexRunError::NonZeroExit {
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_run_output(&stdout))
    }

    pub fn start_new(
        &self,
        label: Option<&str>,
        workspace_root: &Path,
    ) -> Result<SessionHandle, CodexRunError> {
        let prompt = format!(
            "Start a new session for Telegram operator context. Label: {}",
            label.unwrap_or("default")
        );
        let result = self.run(
            &CodexRunRequest {
                prompt,
                resume_target: None,
            },
            workspace_root,
        )?;
        let session_id = result.session_id.ok_or(CodexRunError::MissingSessionId)?;
        Ok(SessionHandle {
            codex_session_id: session_id,
            display_name: label
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("codex-session")
                .to_owned(),
        })
    }

    pub fn resume(
        &self,
        target: &str,
        workspace_root: &Path,
    ) -> Result<SessionHandle, CodexRunError> {
        let result = self.run(
            &CodexRunRequest {
                prompt: "Continue from last context checkpoint".to_owned(),
                resume_target: Some(target.to_owned()),
            },
            workspace_root,
        )?;
        Ok(SessionHandle {
            codex_session_id: result.session_id.unwrap_or_else(|| target.to_owned()),
            display_name: target.to_owned(),
        })
    }
}

fn parse_run_output(stdout: &str) -> CodexRunResult {
    let mut transcript = CodexTranscript::default();
    let mut events = Vec::new();

    for line in stdout.lines() {
        if let ParsedCodexLine::Event(event) = parse_codex_line(line) {
            transcript.apply(event.clone());
            events.push(event);
        }
    }

    CodexRunResult {
        session_id: transcript.session_id().map(str::to_owned),
        transcript,
        events,
    }
}

fn build_exec_args(request: &CodexRunRequest) -> Vec<String> {
    let mut args = vec!["exec".to_owned()];
    if let Some(target) = request.resume_target.as_ref() {
        args.push("resume".to_owned());
        args.push(target.to_owned());
    }
    args.push("--json".to_owned());
    args.push(request.prompt.clone());
    args
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::{CodexExecRunner, CodexRunRequest, build_exec_args, parse_run_output};

    #[test]
    fn builds_resume_args_for_codex_exec_json() {
        let args = build_exec_args(&CodexRunRequest {
            prompt: "hello".to_owned(),
            resume_target: Some("sess-123".to_owned()),
        });

        assert_eq!(args, vec!["exec", "resume", "sess-123", "--json", "hello"]);
    }

    #[test]
    fn parses_session_and_transcript_from_jsonl() {
        let result = parse_run_output(
            "{\"type\":\"thread.started\",\"thread\":{\"id\":\"sess-1\"}}\n{\"type\":\"assistant_message\",\"text\":\"done\"}\n",
        );

        assert_eq!(result.session_id.as_deref(), Some("sess-1"));
        assert_eq!(result.transcript.rendered(), "done");
    }

    #[test]
    fn runs_fake_codex_binary() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("fake-codex.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\necho '{\"type\":\"thread.started\",\"thread\":{\"id\":\"sess-9\"}}'\n",
        )
        .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms).unwrap();
        }

        let runner = CodexExecRunner::with_binary(script_path.to_string_lossy());
        let handle = runner.start_new(Some("demo"), temp.path()).unwrap();
        assert_eq!(handle.codex_session_id, "sess-9");
        assert_eq!(handle.display_name, "demo");
    }
}
