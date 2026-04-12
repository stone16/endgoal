use std::path::Path;
use std::pin::Pin;

use futures::Stream;

use crate::shared::types::{RunEvent, RunInput};

// ---------------------------------------------------------------------------
// RuntimeAdapter trait
// ---------------------------------------------------------------------------

/// A runtime adapter spawns a subprocess and streams events back.
pub trait RuntimeAdapter: Send + Sync {
    fn name(&self) -> &str;

    /// Execute the given input in the scratchpad directory.
    /// Returns a stream of RunEvents (stdout/stderr lines) followed by
    /// the caller handling the terminal event based on exit code.
    fn execute(
        &self,
        run_id: &str,
        input: &RunInput,
        scratchpad: &Path,
    ) -> Pin<Box<dyn Stream<Item = RunEvent> + Send>>;
}

// ---------------------------------------------------------------------------
// EchoAdapter — for testing; spawns `echo <intent>`
// ---------------------------------------------------------------------------

pub struct EchoAdapter;

impl RuntimeAdapter for EchoAdapter {
    fn name(&self) -> &str {
        "echo"
    }

    fn execute(
        &self,
        run_id: &str,
        input: &RunInput,
        scratchpad: &Path,
    ) -> Pin<Box<dyn Stream<Item = RunEvent> + Send>> {
        spawn_process(run_id, "echo", &[&input.intent], scratchpad)
    }
}

// ---------------------------------------------------------------------------
// ClaudeCodeAdapter — spawns `claude --workspace <path>`
// ---------------------------------------------------------------------------

pub struct ClaudeCodeAdapter;

impl RuntimeAdapter for ClaudeCodeAdapter {
    fn name(&self) -> &str {
        "claude"
    }

    fn execute(
        &self,
        run_id: &str,
        _input: &RunInput,
        scratchpad: &Path,
    ) -> Pin<Box<dyn Stream<Item = RunEvent> + Send>> {
        let workspace = scratchpad.to_string_lossy().to_string();
        spawn_process(run_id, "claude", &["--workspace", &workspace], scratchpad)
    }
}

// ---------------------------------------------------------------------------
// CodexAdapter — spawns `codex --workspace <path>`
// ---------------------------------------------------------------------------

pub struct CodexAdapter;

impl RuntimeAdapter for CodexAdapter {
    fn name(&self) -> &str {
        "codex"
    }

    fn execute(
        &self,
        run_id: &str,
        _input: &RunInput,
        scratchpad: &Path,
    ) -> Pin<Box<dyn Stream<Item = RunEvent> + Send>> {
        let workspace = scratchpad.to_string_lossy().to_string();
        spawn_process(run_id, "codex", &["--workspace", &workspace], scratchpad)
    }
}

// ---------------------------------------------------------------------------
// Shared subprocess spawn logic
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicI64, Ordering};

/// Spawn a subprocess and return a stream of RunEvent (stdout/stderr lines).
/// Does NOT emit RunTerminal — the caller is responsible for that.
pub fn spawn_process(
    run_id: &str,
    program: &str,
    args: &[&str],
    working_dir: &Path,
) -> Pin<Box<dyn Stream<Item = RunEvent> + Send>> {
    use async_stream::stream;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command;

    let run_id = run_id.to_string();
    let program = program.to_string();
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let working_dir = working_dir.to_path_buf();

    let seq = std::sync::Arc::new(AtomicI64::new(0));

    Box::pin(stream! {
        let result = Command::new(&program)
            .args(&args)
            .current_dir(&working_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        match result {
            Ok(mut child) => {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();

                // Read stdout and stderr concurrently
                let seq_out = seq.clone();
                let run_id_out = run_id.clone();
                let stdout_handle = tokio::spawn(async move {
                    let mut events = Vec::new();
                    if let Some(stdout) = stdout {
                        let reader = BufReader::new(stdout);
                        let mut lines = reader.lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let s = seq_out.fetch_add(1, Ordering::SeqCst);
                            events.push(RunEvent {
                                run_id: run_id_out.clone(),
                                seq: s,
                                event_type: "stdout".to_string(),
                                data_text: Some(line),
                            });
                        }
                    }
                    events
                });

                let seq_err = seq.clone();
                let run_id_err = run_id.clone();
                let stderr_handle = tokio::spawn(async move {
                    let mut events = Vec::new();
                    if let Some(stderr) = stderr {
                        let reader = BufReader::new(stderr);
                        let mut lines = reader.lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let s = seq_err.fetch_add(1, Ordering::SeqCst);
                            events.push(RunEvent {
                                run_id: run_id_err.clone(),
                                seq: s,
                                event_type: "stderr".to_string(),
                                data_text: Some(line),
                            });
                        }
                    }
                    events
                });

                // Wait for both to finish
                let (stdout_events, stderr_events) = tokio::join!(stdout_handle, stderr_handle);

                // Yield stdout events
                if let Ok(events) = stdout_events {
                    for event in events {
                        yield event;
                    }
                }

                // Yield stderr events
                if let Ok(events) = stderr_events {
                    for event in events {
                        yield event;
                    }
                }

                // Wait for process exit
                let exit_status = child.wait().await;
                let code = exit_status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
                let terminal_status = if code == 0 { "completed" } else { "failed" };
                let s = seq.fetch_add(1, Ordering::SeqCst);

                // Emit a system event with the terminal status
                // The caller will convert this to a RunTerminal
                yield RunEvent {
                    run_id: run_id.clone(),
                    seq: s,
                    event_type: format!("_terminal:{terminal_status}"),
                    data_text: None,
                };
            }
            Err(e) => {
                // Process failed to spawn — emit error event
                yield RunEvent {
                    run_id: run_id.clone(),
                    seq: 0,
                    event_type: "_terminal:failed".to_string(),
                    data_text: Some(format!("spawn error: {e}")),
                };
            }
        }
    })
}

/// Look up a RuntimeAdapter by name.
pub fn get_adapter(runtime: &str) -> Option<Box<dyn RuntimeAdapter>> {
    match runtime {
        "echo" => Some(Box::new(EchoAdapter)),
        "claude" => Some(Box::new(ClaudeCodeAdapter)),
        "codex" => Some(Box::new(CodexAdapter)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests — written FIRST per TDD
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::types::*;
    use futures::StreamExt;

    fn make_test_input(intent: &str) -> RunInput {
        RunInput {
            intent: intent.to_string(),
            acceptance: Acceptance::Prose {
                text: "test".to_string(),
            },
            effective_policy: Policy {
                tokens_max: None,
                iterations_max: None,
                wallclock_max_s: None,
                allowed_tools: None,
                review_required: None,
            },
            parent_context: vec![],
            node_docs: vec![],
        }
    }

    #[test]
    fn test_echo_adapter_name() {
        let adapter = EchoAdapter;
        assert_eq!(adapter.name(), "echo");
    }

    #[test]
    fn test_claude_adapter_name() {
        let adapter = ClaudeCodeAdapter;
        assert_eq!(adapter.name(), "claude");
    }

    #[test]
    fn test_codex_adapter_name() {
        let adapter = CodexAdapter;
        assert_eq!(adapter.name(), "codex");
    }

    #[test]
    fn test_get_adapter_echo() {
        let adapter = get_adapter("echo");
        assert!(adapter.is_some());
        assert_eq!(adapter.unwrap().name(), "echo");
    }

    #[test]
    fn test_get_adapter_claude() {
        let adapter = get_adapter("claude");
        assert!(adapter.is_some());
        assert_eq!(adapter.unwrap().name(), "claude");
    }

    #[test]
    fn test_get_adapter_codex() {
        let adapter = get_adapter("codex");
        assert!(adapter.is_some());
        assert_eq!(adapter.unwrap().name(), "codex");
    }

    #[test]
    fn test_get_adapter_unknown() {
        let adapter = get_adapter("nonexistent");
        assert!(adapter.is_none());
    }

    #[tokio::test]
    async fn test_echo_adapter_produces_stdout_event() {
        let adapter = EchoAdapter;
        let input = make_test_input("hello");
        let tmp = tempfile::tempdir().unwrap();

        let mut stream = adapter.execute("test-1", &input, tmp.path());
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event);
        }

        // Should have at least one stdout event with "hello" and one terminal event
        let stdout_events: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == "stdout")
            .collect();
        assert!(!stdout_events.is_empty(), "expected at least one stdout event");
        assert_eq!(stdout_events[0].data_text.as_deref(), Some("hello"));
        assert_eq!(stdout_events[0].run_id, "test-1");

        // Should have a terminal event indicating completion
        let terminal_events: Vec<_> = events
            .iter()
            .filter(|e| e.event_type.starts_with("_terminal:"))
            .collect();
        assert_eq!(terminal_events.len(), 1);
        assert_eq!(terminal_events[0].event_type, "_terminal:completed");
    }

    #[tokio::test]
    async fn test_echo_adapter_sequential_seq_numbers() {
        let adapter = EchoAdapter;
        let input = make_test_input("line1");
        let tmp = tempfile::tempdir().unwrap();

        let mut stream = adapter.execute("test-seq", &input, tmp.path());
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event);
        }

        // Verify seq numbers are sequential starting from 0
        for (i, event) in events.iter().enumerate() {
            assert_eq!(event.seq, i as i64, "seq mismatch at index {i}");
        }
    }

    #[tokio::test]
    async fn test_nonexistent_binary_produces_failed_terminal() {
        let _input = make_test_input("anything");
        let tmp = tempfile::tempdir().unwrap();

        let mut stream = spawn_process(
            "test-fail",
            "nonexistent_binary_xyz_12345",
            &["arg"],
            tmp.path(),
        );
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event);
        }

        // Should have exactly one terminal:failed event
        assert!(!events.is_empty(), "expected at least one event");
        let last = events.last().unwrap();
        assert_eq!(last.event_type, "_terminal:failed");
        assert!(
            last.data_text.as_ref().unwrap().contains("spawn error"),
            "expected spawn error message, got: {:?}",
            last.data_text
        );
    }

    #[tokio::test]
    async fn test_failing_process_produces_failed_terminal() {
        let _input = make_test_input("anything");
        let tmp = tempfile::tempdir().unwrap();

        // `false` command always exits with code 1
        let mut stream = spawn_process("test-fail-code", "false", &[], tmp.path());
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event);
        }

        let terminal_events: Vec<_> = events
            .iter()
            .filter(|e| e.event_type.starts_with("_terminal:"))
            .collect();
        assert_eq!(terminal_events.len(), 1);
        assert_eq!(terminal_events[0].event_type, "_terminal:failed");
    }

    #[tokio::test]
    async fn test_stderr_captured() {
        let tmp = tempfile::tempdir().unwrap();

        // Use sh -c to write to stderr
        let mut stream = spawn_process(
            "test-stderr",
            "sh",
            &["-c", "echo error_msg >&2"],
            tmp.path(),
        );
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event);
        }

        let stderr_events: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == "stderr")
            .collect();
        assert!(!stderr_events.is_empty(), "expected at least one stderr event");
        assert_eq!(stderr_events[0].data_text.as_deref(), Some("error_msg"));
    }

    #[tokio::test]
    async fn test_multiline_stdout() {
        let tmp = tempfile::tempdir().unwrap();

        let mut stream = spawn_process(
            "test-multi",
            "sh",
            &["-c", "echo line1; echo line2; echo line3"],
            tmp.path(),
        );
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event);
        }

        let stdout_events: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == "stdout")
            .collect();
        assert_eq!(stdout_events.len(), 3);
        assert_eq!(stdout_events[0].data_text.as_deref(), Some("line1"));
        assert_eq!(stdout_events[1].data_text.as_deref(), Some("line2"));
        assert_eq!(stdout_events[2].data_text.as_deref(), Some("line3"));
    }

    #[tokio::test]
    async fn test_working_dir_is_scratchpad() {
        let tmp = tempfile::tempdir().unwrap();

        // pwd should output the scratchpad path
        let mut stream = spawn_process("test-pwd", "pwd", &[], tmp.path());
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event);
        }

        let stdout_events: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == "stdout")
            .collect();
        assert!(!stdout_events.is_empty());
        // On macOS, /tmp may resolve to /private/tmp
        let expected = tmp.path().canonicalize().unwrap();
        let actual_text = stdout_events[0].data_text.as_deref().unwrap();
        let actual = std::path::PathBuf::from(actual_text).canonicalize().unwrap();
        assert_eq!(actual, expected);
    }
}
