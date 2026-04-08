use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tempfile::TempDir;
use vibes_codex::{
    CodexEvent, CodexExecRunner, CodexRunControl, CodexRunError, CodexRunRequest, ParsedCodexLine,
};

#[test]
fn requires_explicit_successful_completion() {
    let temp = TempDir::new().unwrap();
    let script_path = write_script(
        temp.path(),
        "fake-codex.sh",
        "#!/bin/sh\necho '{\"type\":\"thread.started\",\"thread\":{\"id\":\"sess-1\"}}'\n",
    );
    let runner = CodexExecRunner::with_binary(script_path.to_string_lossy());

    let error = runner
        .run(
            &CodexRunRequest {
                prompt: "hello".to_owned(),
                resume_target: None,
            },
            temp.path(),
        )
        .unwrap_err();

    assert!(matches!(
        error,
        CodexRunError::MissingSuccessfulCompletion { conclusion: None }
    ));
}

#[test]
fn streams_events_before_process_exit() {
    let temp = TempDir::new().unwrap();
    let script_path = write_script(
        temp.path(),
        "stream-codex.sh",
        "#!/bin/sh\nprintf '{\"type\":\"thread.started\",\"thread\":{\"id\":\"sess-9\"}}\\n'\nsleep 0.2\nprintf 'plain-noise\\n'\nsleep 0.2\nprintf '{\"type\":\"assistant_message\",\"text\":\"done\"}\\n'\nsleep 0.2\nprintf '{\"type\":\"turn.completed\",\"success\":true}\\n'\n",
    );
    let runner = CodexExecRunner::with_binary(script_path.to_string_lossy());
    let control = CodexRunControl::default();
    let (tx, rx) = mpsc::channel();

    let join = thread::spawn(move || {
        runner
            .run_with_handler(
                &CodexRunRequest {
                    prompt: "hello".to_owned(),
                    resume_target: None,
                },
                temp.path(),
                &control,
                |parsed| {
                    tx.send(parsed.clone()).unwrap();
                },
            )
            .unwrap()
    });

    assert_eq!(
        recv_until(&rx, |line| {
            matches!(
                line,
                ParsedCodexLine::Event(CodexEvent::SessionIdentified { session_id })
                    if session_id == "sess-9"
            )
        }),
        ParsedCodexLine::Event(CodexEvent::SessionIdentified {
            session_id: "sess-9".to_owned(),
        })
    );
    assert_eq!(
        recv_until(
            &rx,
            |line| matches!(line, ParsedCodexLine::Noise(text) if text == "plain-noise")
        ),
        ParsedCodexLine::Noise("plain-noise".to_owned())
    );
    let result = join.join().unwrap();
    assert_eq!(result.session_id.as_deref(), Some("sess-9"));
    assert_eq!(result.transcript.rendered(), "done");
}

#[test]
fn cancels_running_process() {
    let temp = TempDir::new().unwrap();
    let script_path = write_script(
        temp.path(),
        "slow-codex.sh",
        "#!/bin/sh\nprintf '{\"type\":\"thread.started\",\"thread\":{\"id\":\"sess-cancel\"}}\\n'\nsleep 5\n",
    );
    let runner = CodexExecRunner::with_binary(script_path.to_string_lossy());
    let control = CodexRunControl::default();
    let cancel = control.clone();

    let join = thread::spawn(move || {
        runner.run_with_handler(
            &CodexRunRequest {
                prompt: "hello".to_owned(),
                resume_target: None,
            },
            temp.path(),
            &control,
            |_| {},
        )
    });

    thread::sleep(Duration::from_millis(150));
    cancel.cancel();
    assert!(matches!(
        join.join().unwrap(),
        Err(CodexRunError::Cancelled)
    ));
}

#[test]
fn start_new_requires_success_event_and_session_id() {
    let temp = TempDir::new().unwrap();
    let script_path = write_script(
        temp.path(),
        "fake-codex.sh",
        "#!/bin/sh\necho '{\"type\":\"thread.started\",\"thread\":{\"id\":\"sess-9\"}}'\necho '{\"type\":\"turn.completed\",\"success\":true}'\n",
    );
    let runner = CodexExecRunner::with_binary(script_path.to_string_lossy());
    let handle = runner.start_new(Some("demo"), temp.path()).unwrap();
    assert_eq!(handle.codex_session_id, "sess-9");
    assert_eq!(handle.display_name, "demo");
}

fn recv_until<F>(rx: &mpsc::Receiver<ParsedCodexLine>, predicate: F) -> ParsedCodexLine
where
    F: Fn(&ParsedCodexLine) -> bool,
{
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let line = rx.recv_timeout(remaining).unwrap();
        if predicate(&line) {
            return line;
        }
    }
}

fn write_script(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let script_path = dir.join(name);
    fs::write(&script_path, contents).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    script_path
}
