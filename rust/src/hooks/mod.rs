use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::HookCommand;
use crate::error::{WsError, WsResult};

pub struct HookContext<'a> {
    pub hook_kind: &'static str,
    pub path: &'a str,
    pub work_dir: &'a Path,
}

pub fn run_hook(cmd: &HookCommand, input: &str, ctx: &HookContext<'_>) -> WsResult<String> {
    let mut command = Command::new(&cmd.command[0]);
    command
        .args(&cmd.command[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(ctx.work_dir)
        .env("WS_HOOK", ctx.hook_kind)
        .env("WS_PATH", ctx.path);

    let mut child = command
        .spawn()
        .map_err(|e| WsError::Other(format!("failed to spawn hook command: {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .map_err(|e| WsError::Other(format!("failed to write hook stdin: {e}")))?;
    }

    let child = Arc::new(Mutex::new(Some(child)));
    let child_for_wait = Arc::clone(&child);
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut guard = child_for_wait.lock().expect("hook child lock");
        if let Some(child) = guard.take() {
            let _ = tx.send(child.wait_with_output());
        }
    });

    let timeout = Duration::from_millis(cmd.timeout_ms);
    let output = match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Err(WsError::Other(format!("failed to wait for hook command: {e}")));
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            let mut guard = child.lock().expect("hook child lock");
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            return Err(WsError::Other(format!(
                "hook command timed out after {} ms",
                cmd.timeout_ms
            )));
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            return Err(WsError::Other("hook command waiter exited unexpectedly".to_string()));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WsError::Other(format!(
            "hook command failed with status {}: {}",
            output.status, stderr.trim()
        )));
    }

    String::from_utf8(output.stdout).map_err(|e| {
        WsError::Other(format!("hook command produced invalid UTF-8 output: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn ctx(work_dir: &Path) -> HookContext<'_> {
        HookContext {
            hook_kind: "read",
            path: "test.txt",
            work_dir,
        }
    }

    fn passthrough_cmd() -> HookCommand {
        HookCommand {
            command: vec![
                "python3".to_string(),
                "-c".to_string(),
                "import sys; print(sys.stdin.read(), end='')".to_string(),
            ],
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn run_hook_passthrough() {
        let tmp = TempDir::new().unwrap();
        let out = run_hook(&passthrough_cmd(), "hello\n", &ctx(tmp.path())).unwrap();
        assert_eq!(out, "hello\n");
    }

    #[test]
    fn run_hook_empty_input() {
        let tmp = TempDir::new().unwrap();
        let out = run_hook(&passthrough_cmd(), "", &ctx(tmp.path())).unwrap();
        assert_eq!(out, "");
    }

    #[test]
    fn run_hook_nonzero_exit_fails() {
        let tmp = TempDir::new().unwrap();
        let cmd = HookCommand {
            command: vec!["python3".to_string(), "-c".to_string(), "import sys; sys.exit(2)".to_string()],
            timeout_ms: 5_000,
        };
        let err = run_hook(&cmd, "x", &ctx(tmp.path())).unwrap_err();
        assert!(matches!(err, WsError::Other(_)));
    }

    #[test]
    fn run_hook_timeout_fails() {
        let tmp = TempDir::new().unwrap();
        let cmd = HookCommand {
            command: vec!["sleep".to_string(), "5".to_string()],
            timeout_ms: 100,
        };
        let err = run_hook(&cmd, "", &ctx(tmp.path())).unwrap_err();
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn run_hook_sets_env_vars() {
        let tmp = TempDir::new().unwrap();
        let cmd = HookCommand {
            command: vec![
                "python3".to_string(),
                "-c".to_string(),
                "import os, sys; print(os.environ['WS_HOOK'] + ':' + os.environ['WS_PATH'], end='')".to_string(),
            ],
            timeout_ms: 5_000,
        };
        let out = run_hook(&cmd, "", &ctx(tmp.path())).unwrap();
        assert_eq!(out, "read:test.txt");
        let _ = PathBuf::from("unused");
    }
}
