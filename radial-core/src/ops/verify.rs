//! Runs a [`crate::models::Contract`]'s `verify_cmd` as a plain child process.
//!
//! This is the one piece of the verify feature that genuinely needs a
//! timeout and live process control, so it lives apart from `ops::task`,
//! which only ever touches the [`crate::db::Database`]. Nothing here reads
//! or writes `.radial/`.

use std::io::Read;
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Serialize;

/// How long a verify command is allowed to run before it's killed and
/// treated as a failure. Not yet exposed as a setting; documented here as
/// the single place that would need to change.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Tail of captured output kept per stream, in bytes, so a noisy command
/// can't bloat a task's comment history.
const TAIL_BYTES: usize = 4096;

/// How often the timeout loop polls the child for exit, while it waits.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The outcome of running a `verify_cmd` once.
#[derive(Debug, Serialize)]
pub struct VerifyResult {
    /// Whether the command exited zero within the timeout.
    pub passed: bool,
    /// The process's exit code, or `None` if it was killed for timing out.
    pub exit_code: Option<i32>,
    /// The last [`TAIL_BYTES`] of stdout.
    pub stdout_tail: String,
    /// The last [`TAIL_BYTES`] of stderr.
    pub stderr_tail: String,
    /// Wall-clock time the command ran for.
    pub duration_ms: u64,
    /// Whether the command was killed for exceeding its timeout.
    pub timed_out: bool,
}

#[cfg(not(windows))]
fn spawn_shell(cmd: &str) -> std::io::Result<Child> {
    Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

#[cfg(windows)]
fn spawn_shell(cmd: &str) -> std::io::Result<Child> {
    Command::new("cmd")
        .args(["/C", cmd])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

/// Reads a pipe to completion on its own thread, so neither stream can fill
/// up and deadlock the child while the other is being read.
fn read_all_lossy<R: Read + Send + 'static>(mut pipe: R) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// The last `max_bytes` of `s`, cut on a UTF-8 character boundary.
fn tail(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let cut = s.len() - max_bytes;
    let cut = (cut..=s.len())
        .find(|&i| s.is_char_boundary(i))
        .unwrap_or(s.len());
    s[cut..].to_string()
}

/// Runs `cmd` through the platform shell, inheriting this process's working
/// directory and environment (that inheritance is the point: a verify
/// command should see exactly what the agent that's completing the task
/// sees). No `.radial/` lock is touched here — the caller is responsible
/// for releasing it first, per `ROADMAP.md`'s executable-verification item.
pub fn run_cmd(cmd: &str, timeout: Duration) -> Result<VerifyResult> {
    let start = Instant::now();
    let mut child = spawn_shell(cmd).context("failed to spawn verify command")?;

    let stdout: ChildStdout = child.stdout.take().expect("stdout was piped");
    let stderr: ChildStderr = child.stderr.take().expect("stderr was piped");
    let stdout_handle = read_all_lossy(stdout);
    let stderr_handle = read_all_lossy(stderr);

    let (status, timed_out): (Option<ExitStatus>, bool) = loop {
        if let Some(status) = child.try_wait().context("failed to poll verify command")? {
            break (Some(status), false);
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            break (child.wait().ok(), true);
        }
        thread::sleep(POLL_INTERVAL);
    };

    let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let stdout_tail = tail(&stdout_handle.join().unwrap_or_default(), TAIL_BYTES);
    let stderr_tail = tail(&stderr_handle.join().unwrap_or_default(), TAIL_BYTES);
    let exit_code = status.and_then(|s| s.code());
    let passed = !timed_out && status.is_some_and(|s| s.success());

    Ok(VerifyResult {
        passed,
        exit_code,
        stdout_tail,
        stderr_tail,
        duration_ms,
        timed_out,
    })
}

#[cfg(test)]
mod tests {
    use super::{Duration, run_cmd, tail};

    #[test]
    fn passes_on_zero_exit() {
        let result = run_cmd("exit 0", Duration::from_secs(5)).unwrap();
        assert!(result.passed);
        assert_eq!(result.exit_code, Some(0));
        assert!(!result.timed_out);
    }

    #[test]
    fn fails_on_nonzero_exit() {
        let result = run_cmd("exit 7", Duration::from_secs(5)).unwrap();
        assert!(!result.passed);
        assert_eq!(result.exit_code, Some(7));
        assert!(!result.timed_out);
    }

    #[test]
    fn captures_stdout_and_stderr() {
        let result = run_cmd("echo out; echo err >&2", Duration::from_secs(5)).unwrap();
        assert_eq!(result.stdout_tail.trim(), "out");
        assert_eq!(result.stderr_tail.trim(), "err");
    }

    #[test]
    fn kills_and_reports_timeout() {
        let result = run_cmd("sleep 5", Duration::from_millis(100)).unwrap();
        assert!(!result.passed);
        assert!(result.timed_out);
        assert_eq!(result.exit_code, None);
    }

    #[test]
    fn inherits_environment() {
        // SAFETY: test-local var, no other test reads or writes it.
        unsafe {
            std::env::set_var("RADIAL_VERIFY_TEST_VAR", "present");
        }
        let result = run_cmd(
            "[ \"$RADIAL_VERIFY_TEST_VAR\" = \"present\" ]",
            Duration::from_secs(5),
        )
        .unwrap();
        unsafe {
            std::env::remove_var("RADIAL_VERIFY_TEST_VAR");
        }
        assert!(result.passed);
    }

    #[test]
    fn tail_keeps_last_bytes_on_a_char_boundary() {
        let s = "abcdefg";
        assert_eq!(tail(s, 100), "abcdefg");
        assert_eq!(tail(s, 3), "efg");
    }
}
