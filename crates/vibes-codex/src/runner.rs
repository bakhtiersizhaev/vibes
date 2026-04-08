use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use thiserror::Error;
use vibes_core::SessionHandle;

use crate::{CodexEvent, CodexTranscript, ParsedCodexLine, RunConclusion, parse_codex_line};

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

#[derive(Debug, Clone, Default)]
pub struct CodexRunControl {
    cancelled: Arc<AtomicBool>,
}

impl CodexRunControl {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
pub struct CodexExecRunner {
    binary: String,
}

#[derive(Debug, Error)]
pub enum CodexRunError {
    #[error("failed to spawn codex: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("codex run cancelled")]
    Cancelled,
    #[error("codex exited with non-zero code {code:?}: {stderr}")]
    NonZeroExit { code: Option<i32>, stderr: String },
    #[error("codex exited without explicit successful completion: {conclusion:?}")]
    MissingSuccessfulCompletion { conclusion: Option<RunConclusion> },
    #[error("codex output did not include session id")]
    MissingSessionId,
}

enum StreamMessage {
    Stdout(String),
    Stderr(String),
    ReaderFailed(String),
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
        self.run_with_handler(request, cwd, &CodexRunControl::default(), |_| {})
    }

    pub fn run_with_handler<F>(
        &self,
        request: &CodexRunRequest,
        cwd: &Path,
        control: &CodexRunControl,
        mut on_line: F,
    ) -> Result<CodexRunResult, CodexRunError>
    where
        F: FnMut(&ParsedCodexLine),
    {
        let mut child = Command::new(&self.binary)
            .args(build_exec_args(request))
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().ok_or_else(|| pipe_error("stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| pipe_error("stderr"))?;
        let (tx, rx) = mpsc::channel();

        spawn_reader(stdout, tx.clone(), true);
        spawn_reader(stderr, tx, false);

        let mut transcript = CodexTranscript::default();
        let mut events = Vec::new();
        let mut stderr_log = Vec::new();

        loop {
            if control.is_cancelled() {
                terminate(&mut child)?;
                let _ = child.wait();
                return Err(CodexRunError::Cancelled);
            }

            match rx.recv_timeout(Duration::from_millis(25)) {
                Ok(StreamMessage::Stdout(line)) => {
                    apply_line(&line, &mut transcript, &mut events, &mut on_line)
                }
                Ok(StreamMessage::Stderr(line) | StreamMessage::ReaderFailed(line)) => {
                    stderr_log.push(line)
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {}
            }

            if let Some(status) = child.try_wait()? {
                while let Ok(message) = rx.try_recv() {
                    match message {
                        StreamMessage::Stdout(line) => {
                            apply_line(&line, &mut transcript, &mut events, &mut on_line)
                        }
                        StreamMessage::Stderr(line) | StreamMessage::ReaderFailed(line) => {
                            stderr_log.push(line)
                        }
                    }
                }
                if control.is_cancelled() {
                    return Err(CodexRunError::Cancelled);
                }
                if !status.success() {
                    return Err(CodexRunError::NonZeroExit {
                        code: status.code(),
                        stderr: stderr_log.join("\n").trim().to_owned(),
                    });
                }
                let result = CodexRunResult {
                    session_id: transcript.session_id().map(str::to_owned),
                    transcript,
                    events,
                };
                if result.transcript.conclusion() != Some(RunConclusion::Success) {
                    return Err(CodexRunError::MissingSuccessfulCompletion {
                        conclusion: result.transcript.conclusion(),
                    });
                }
                return Ok(result);
            }
        }
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

fn apply_line<F>(
    line: &str,
    transcript: &mut CodexTranscript,
    events: &mut Vec<CodexEvent>,
    on_line: &mut F,
) where
    F: FnMut(&ParsedCodexLine),
{
    let parsed = parse_codex_line(line);
    if let ParsedCodexLine::Event(event) = &parsed {
        transcript.apply(event.clone());
        events.push(event.clone());
    }
    on_line(&parsed);
}

fn spawn_reader<T>(stream: T, tx: Sender<StreamMessage>, stdout: bool)
where
    T: std::io::Read + Send + 'static,
{
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim_end_matches(['\r', '\n']).to_owned();
                    let message = if stdout {
                        StreamMessage::Stdout(trimmed)
                    } else {
                        StreamMessage::Stderr(trimmed)
                    };
                    if tx.send(message).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let side = if stdout { "stdout" } else { "stderr" };
                    let _ = tx.send(StreamMessage::ReaderFailed(format!(
                        "failed reading codex {side}: {error}"
                    )));
                    break;
                }
            }
        }
    });
}

fn terminate(child: &mut Child) -> Result<(), CodexRunError> {
    match child.kill() {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => Ok(()),
        Err(error) => Err(CodexRunError::Spawn(error)),
    }
}

fn pipe_error(name: &str) -> std::io::Error {
    std::io::Error::other(format!("missing child {name} pipe"))
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
    use super::{CodexRunRequest, build_exec_args};

    #[test]
    fn builds_resume_args_for_codex_exec_json() {
        let args = build_exec_args(&CodexRunRequest {
            prompt: "hello".to_owned(),
            resume_target: Some("sess-123".to_owned()),
        });

        assert_eq!(args, vec!["exec", "resume", "sess-123", "--json", "hello"]);
    }
}
