//! gitx — faithful 1:1 port of Go package `gitx` (pkg/gitx).
//!
//! Runs `git` in a working directory and returns trimmed stdout, plus a test
//! `Fake` Runner and an `is_repo` helper.
//!
//! Design decisions (recorded in PORT-GLOSSARY):
//! - The Go `Runner` interface method `Run(ctx, dir, args...) (string, error)`
//!   drops the `context.Context` param — no cancellation semantics are tested,
//!   and omitting it consistently across all callers is idiomatic Rust.
//! - `run(&self, ...)` takes `&self` (not `&mut self`) so a `&dyn Runner` can be
//!   held and called repeatedly by downstream modules. `Fake` uses interior
//!   mutability (`RefCell`) to record calls, mirroring Go's pointer-receiver
//!   mutation.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::process::Command;

/// Error returned by a git invocation.
///
/// Its `Display` reproduces the Go error string faithfully:
/// `git <args joined by space> (in <dir>): <cause>: <trimmed stderr>`.
#[derive(Debug)]
pub struct GitError {
    /// The git subcommand args (without the prepended `-c safe.directory=*`).
    pub args: Vec<String>,
    /// The working directory the command ran in.
    pub dir: String,
    /// The underlying cause (exit status text or OS spawn error).
    pub cause: String,
    /// Trimmed stderr from git.
    pub stderr: String,
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Go: fmt.Errorf("git %s (in %s): %w: %s", join(args," "), dir, err, trim(stderr))
        write!(
            f,
            "git {} (in {}): {}: {}",
            self.args.join(" "),
            self.dir,
            self.cause,
            self.stderr
        )
    }
}

impl std::error::Error for GitError {}

/// Runner runs git in a given working directory and returns trimmed stdout.
pub trait Runner {
    fn run(&self, dir: &str, args: &[&str]) -> Result<String, GitError>;
}

/// A `Runner` backed by the system `git` binary.
pub struct ExecRunner;

/// Returns a `Runner` backed by the system git binary (Go `New`).
pub fn new_runner() -> ExecRunner {
    ExecRunner
}

impl ExecRunner {
    pub fn new() -> ExecRunner {
        ExecRunner
    }
}

impl Default for ExecRunner {
    fn default() -> Self {
        ExecRunner
    }
}

impl Runner for ExecRunner {
    fn run(&self, dir: &str, args: &[&str]) -> Result<String, GitError> {
        // Prepend -c safe.directory=* so git operates on repos copied across
        // machines or living on external/synced volumes (different ownership),
        // which would otherwise fail with "detected dubious ownership". This
        // tool's whole purpose is consolidating such cross-machine copies, so
        // this is the normal case.
        let mut cmd = Command::new("git");
        cmd.arg("-c").arg("safe.directory=*");
        cmd.args(args);
        cmd.current_dir(dir);

        let owned_args: Vec<String> = args.iter().map(|s| s.to_string()).collect();

        match cmd.output() {
            Ok(output) => {
                if output.status.success() {
                    // Go: strings.TrimSpace(out.String()) — trims BOTH ends.
                    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    Err(GitError {
                        args: owned_args,
                        dir: dir.to_string(),
                        cause: exit_status_string(&output.status),
                        stderr,
                    })
                }
            }
            // Spawn/OS-level failure (e.g. git not found): stderr is empty,
            // matching Go's TrimSpace of an empty buffer.
            Err(e) => Err(GitError {
                args: owned_args,
                dir: dir.to_string(),
                cause: e.to_string(),
                stderr: String::new(),
            }),
        }
    }
}

/// Formats a non-success exit status similarly to Go's `*exec.ExitError`,
/// e.g. `exit status 1`.
fn exit_status_string(status: &std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit status {}", code),
        None => "signal: killed".to_string(),
    }
}

/// Reports whether `dir` contains a `.git` entry (dir or file, for worktrees).
/// Go uses `os.Stat` on the joined path; `Path::exists` matches (follows to a
/// dir OR a file).
pub fn is_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Fake is a `Runner` for tests. It matches on args joined by " ".
pub struct Fake {
    /// Canned responses keyed by the joined args string.
    pub responses: HashMap<String, String>,
    /// Canned errors keyed by the joined args string.
    pub errs: HashMap<String, String>,
    /// Records every call's joined-args key, in order (interior-mutable so
    /// `run(&self)` works behind a `&dyn Runner`).
    pub calls: RefCell<Vec<String>>,
}

impl Fake {
    /// Returns an initialized Fake (Go `NewFake`).
    pub fn new() -> Fake {
        Fake {
            responses: HashMap::new(),
            errs: HashMap::new(),
            calls: RefCell::new(Vec::new()),
        }
    }

    /// Builder helper: register a canned response for a joined-args key.
    pub fn with_response(mut self, key: &str, value: &str) -> Fake {
        self.responses.insert(key.to_string(), value.to_string());
        self
    }

    /// Builder helper: register a canned error for a joined-args key.
    pub fn with_error(mut self, key: &str, value: &str) -> Fake {
        self.errs.insert(key.to_string(), value.to_string());
        self
    }

    /// Returns a snapshot of the recorded call keys.
    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl Default for Fake {
    fn default() -> Self {
        Fake::new()
    }
}

impl Runner for Fake {
    fn run(&self, _dir: &str, args: &[&str]) -> Result<String, GitError> {
        let key = args.join(" ");
        self.calls.borrow_mut().push(key.clone());
        if let Some(err) = self.errs.get(&key) {
            return Err(GitError {
                args: args.iter().map(|s| s.to_string()).collect(),
                dir: _dir.to_string(),
                cause: err.clone(),
                stderr: String::new(),
            });
        }
        // Go: strings.TrimSpace(f.Responses[key]) — missing key yields "" then
        // trimmed to "".
        Ok(self
            .responses
            .get(&key)
            .map(|v| v.trim().to_string())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Faithful port of Go TestFakeReturnsCannedOutput.
    #[test]
    fn fake_returns_canned_output() {
        let f = Fake::new().with_response("rev-parse HEAD", "abc123\n");
        let got = f.run("/repo", &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(got, "abc123"); // run trims trailing whitespace
        let calls = f.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], "rev-parse HEAD");
    }

    fn unique_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!(
            "reposmerge-gitx-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn is_repo_true_false() {
        let dir = unique_dir("isrepo");
        assert!(!is_repo(&dir), "empty dir must not be a repo");
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        assert!(is_repo(&dir), "dir with .git must be a repo");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ExecRunner error path: running git in a non-repo dir fails, and the
    // GitError Display reproduces the Go framing including the dir. Needs git on
    // PATH (conductor has it).
    #[test]
    fn exec_runner_errors_in_non_repo() {
        let dir = unique_dir("nonrepo");
        let dir_str = dir.to_string_lossy().to_string();
        let res = new_runner().run(&dir_str, &["rev-parse", "--verify", "HEAD"]);
        assert!(res.is_err(), "rev-parse in a non-repo must error");
        let msg = res.unwrap_err().to_string();
        assert!(
            msg.contains("git rev-parse --verify HEAD (in "),
            "unexpected error framing: {msg}"
        );
        assert!(msg.contains(&dir_str), "error should name the dir: {msg}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Fake error path: a registered error key returns Err (key = args joined by
    // space).
    #[test]
    fn fake_error_path() {
        let f = Fake::new().with_error("boom", "x");
        let res = f.run("/repo", &["boom"]);
        assert!(res.is_err(), "expected canned error for key 'boom'");
    }
}
