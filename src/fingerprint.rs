//! Faithful 1:1 port of Go `internal/fingerprint/fingerprint.go`.
//!
//! `compute` fills a [`Copy`]'s [`Fingerprint`] from git output produced by a
//! [`Runner`]. Everything except `worktree_size`/`dir_mtime` is populated here
//! (matching the Go doc comment).
//!
//! Design decisions (see PORT-GLOSSARY):
//! - `context.Context` is DROPPED from `Runner`, so `compute` takes no ctx
//!   (mirrors every downstream caller).
//! - Go's `safe(s, _)` returns the string EVEN ON ERROR (the string is "" on
//!   error), so it is exactly `Result::unwrap_or_default`.
//! - `head` uses Go's `fp.Head, _ =` — keeps whatever string (""" on error) →
//!   `unwrap_or_default` again.

use chrono::{DateTime, Utc};

use crate::gitx::{GitError, Runner};
use crate::model::{Branch, Copy, Fingerprint};

/// Fills `c.fp` from git (everything except worktree_size/dir_mtime).
///
/// Faithful to Go `Compute`. Returns `Ok(())` always in practice — git errors
/// are individually swallowed (via `safe`/`_`) exactly as the source does; the
/// `Result` signature is kept for parity with the Go `error` return.
pub fn compute(r: &dyn Runner, c: &mut Copy) -> Result<(), GitError> {
    let mut fp = Fingerprint::default();

    // "" for empty repo (Go: fp.Head, _ = ...).
    fp.head = r.run(&c.path, &["rev-parse", "HEAD"]).unwrap_or_default();

    fp.root_commits = lines(&safe(r.run(
        &c.path,
        &["rev-list", "--max-parents=0", "--all"],
    )));
    fp.root_commits.sort();

    fp.all_commits = lines(&safe(r.run(&c.path, &["rev-list", "--all"])));
    fp.all_commits.sort();
    fp.commit_count = fp.all_commits.len() as i64;

    for ln in lines(&safe(r.run(
        &c.path,
        &[
            "for-each-ref",
            "--format=%(refname:short) %(objectname)",
            "refs/heads",
        ],
    ))) {
        // Go strings.Cut on first ' ': skip lines without a space.
        if let Some((name, tip)) = ln.split_once(' ') {
            fp.branches.push(Branch {
                name: name.to_string(),
                tip: tip.to_string(),
            });
        }
    }

    for ln in lines(&safe(r.run(&c.path, &["status", "--porcelain"]))) {
        if ln.starts_with("??") {
            fp.untracked_count += 1;
        } else if !ln.trim().is_empty() {
            fp.dirty_count += 1;
        }
    }

    fp.stash_count = lines(&safe(r.run(&c.path, &["stash", "list"]))).len() as i64;

    // ahead/behind vs upstream of current branch.
    if let Ok(up) = r.run(
        &c.path,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    ) {
        if !up.is_empty() {
            let range = format!("{}...HEAD", up);
            if let Ok(out) = r.run(&c.path, &["rev-list", "--count", "--left-right", &range]) {
                // Go strings.Cut on first '\t': behind=left, ahead=right.
                if let Some((b, a)) = out.split_once('\t') {
                    // Go strconv.Atoi err ignored via `_` → 0 on failure.
                    fp.behind = b.trim().parse().unwrap_or(0);
                    fp.ahead = a.trim().parse().unwrap_or(0);
                }
            }
        }
    }

    if let Ok(iso) = r.run(&c.path, &["log", "-1", "--format=%cI"]) {
        if !iso.is_empty() {
            // Go time.Parse(time.RFC3339, iso). On parse error leave zero-time.
            if let Ok(t) = DateTime::parse_from_rfc3339(&iso) {
                fp.last_commit = t.with_timezone(&Utc);
            }
        }
    }

    c.fp = fp;
    Ok(())
}

/// Go `safe(s string, _ error) string` — returns the string, discarding the
/// error. Since `run` yields "" on error, this is `unwrap_or_default`.
fn safe(r: Result<String, GitError>) -> String {
    r.unwrap_or_default()
}

/// Go `lines`: trim the whole string; "" → empty vec; else split on '\n'.
fn lines(s: &str) -> Vec<String> {
    let s = s.trim();
    if s.is_empty() {
        return Vec::new();
    }
    s.split('\n').map(|l| l.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gitx::Fake;

    // Faithful port of Go TestComputeParsesGitOutput.
    #[test]
    fn test_compute_parses_git_output() {
        let f = Fake::new()
            .with_response("rev-parse HEAD", "deadbeef")
            .with_response("rev-list --max-parents=0 --all", "root1\nroot2")
            .with_response("rev-list --all", "deadbeef\nc2\nc3")
            .with_response(
                "for-each-ref --format=%(refname:short) %(objectname) refs/heads",
                "main deadbeef\ndev c2",
            )
            .with_response("status --porcelain", " M a.go\n?? new.txt\n M b.go")
            .with_response("stash list", "stash@{0}: WIP")
            .with_response("rev-list --count --left-right origin/main...HEAD", "2\t3")
            .with_response("log -1 --format=%cI", "2026-06-20T10:00:00-03:00")
            .with_response("rev-parse --abbrev-ref HEAD", "main")
            .with_response(
                "rev-parse --abbrev-ref --symbolic-full-name @{u}",
                "origin/main",
            );

        let mut c = Copy {
            path: "/repo".to_string(),
            ..Default::default()
        };
        compute(&f, &mut c).expect("compute should not error");
        let fp = &c.fp;
        assert_eq!(fp.head, "deadbeef");
        assert_eq!(fp.commit_count, 3);
        assert_eq!(fp.dirty_count, 2);
        assert_eq!(fp.untracked_count, 1);
        assert_eq!(fp.stash_count, 1);
        assert_eq!(fp.ahead, 3);
        assert_eq!(fp.behind, 2);
        assert_eq!(fp.root_commits.len(), 2);
        assert_eq!(fp.branches.len(), 2);
    }
}
