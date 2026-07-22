//! consolidate — faithful 1:1 port of Go package `consolidate`
//! (internal/consolidate). Orchestration core: executes a `Plan`, read-only on
//! sources, writing only under a destination tree (+ `_quarantine`).
//!
//! Design decisions (recorded in PORT-GLOSSARY):
//! - Go's `context.Context` param is DROPPED (no cancellation tested), matching
//!   the `gitx::Runner` contract used across the port.
//! - Go's `log/slog` side-effect logging is DROPPED entirely — it is not part of
//!   any tested behavior and adding a logging crate would be an unjustified dep.
//! - Go returns a single `error` that wraps both `io` errors (from safety) and
//!   git errors (from the Runner). Rust models this with a small module `Error`
//!   enum wrapping `std::io::Error` (safety: copy_tree_atomic/tree_hash) and
//!   `gitx::GitError` (Runner). `Display` reproduces the Go `fmt.Errorf` wrap
//!   messages verbatim.

use std::collections::HashSet;
use std::fmt;

use crate::gitx::{GitError, Runner};
use crate::model::{ApplyResult, Plan};
use crate::safety::{copy_tree_atomic, tree_hash};

/// DefaultExcludes are regenerable dirs skipped unless `include_generated`.
/// Order preserved from Go `DefaultExcludes`.
pub static DEFAULT_EXCLUDES: &[&str] = &[
    "node_modules",
    "vendor",
    "dist",
    ".next",
    "build",
    ".gradle",
    "target",
    "__pycache__",
];

/// Convenience owned form of [`DEFAULT_EXCLUDES`].
pub fn default_excludes() -> Vec<String> {
    DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect()
}

/// Options configures [`apply`].
///
/// `exclude_dirs` is `Option<Vec<String>>` to mirror Go's nil-vs-non-nil slice
/// distinction that `excludes()` depends on: `None` (Go nil) → `DefaultExcludes`,
/// `Some(v)` (Go non-nil, even empty) → `v`.
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub dest: String,
    pub dry_run: bool,
    pub include_generated: bool,
    pub exclude_dirs: Option<Vec<String>>,
}

impl Options {
    fn excludes(&self) -> Vec<String> {
        if self.include_generated {
            return Vec::new(); // Go: nil
        }
        match &self.exclude_dirs {
            Some(v) => v.clone(),
            None => default_excludes(),
        }
    }
}

/// Error returned by [`apply`], wrapping the two failure sources (safety I/O and
/// git). `Display` reproduces the Go `fmt.Errorf` wrap strings verbatim.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Git(GitError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{}", e),
            Error::Git(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Git(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<GitError> for Error {
    fn from(e: GitError) -> Self {
        Error::Git(e)
    }
}

/// Executes the plan. Read-only on sources; writes only under DestPath and
/// `DestPath/_quarantine`. Faithful port of Go `Apply` (ctx dropped).
pub fn apply(r: &dyn Runner, p: &Plan, opts: &Options) -> Result<ApplyResult, Error> {
    let mut res = ApplyResult::default();
    let ex = opts.excludes();
    // guard against Strategy-C dest collisions
    let mut seen: HashSet<String> = HashSet::new();

    for d in &p.decisions {
        // 0. disambiguate two different projects that resolve to the same dest path
        let mut dest = d.dest_path.clone();
        if seen.contains(&dest) {
            let mut suffix = d.canonical.machine.clone();
            if let Some(rc0) = d.canonical.fp.root_commits.first() {
                if rc0.len() >= 7 {
                    suffix = rc0[..7].to_string();
                }
            }
            dest = format!("{}-{}", dest, suffix);
        }
        seen.insert(dest.clone());

        // 1. canonical working tree
        let already_done = already_consolidated(&d.canonical.path, &dest, &canonical_excludes(&ex))
            .map_err(|e| wrap(format!("check canonical {}", d.group.repo_name), e))?;
        if already_done {
            res.skipped += 1;
        } else {
            let sk = copy_tree_atomic(&d.canonical.path, &dest, &ex, opts.dry_run)
                .map_err(|e| wrap_io(format!("copy canonical {}", d.group.repo_name), e))?;
            res.skipped_files.extend(sk);
            res.copied += 1;
        }

        // 2. quarantine divergent copies (Strategy A/C)
        for q in &d.quarantine {
            let mut qdest = q.dest_path.clone();
            if seen.contains(&qdest) {
                let mut suffix = q.copy.machine.clone();
                if let Some(rc0) = q.copy.fp.root_commits.first() {
                    if rc0.len() >= 7 {
                        suffix = rc0[..7].to_string();
                    }
                }
                qdest = format!("{}-{}", qdest, suffix);
            }
            seen.insert(qdest.clone());

            let q_already = already_consolidated(&q.copy.path, &qdest, &ex).map_err(|e| {
                wrap(
                    format!("check quarantine {}/{}", d.group.repo_name, q.copy.machine),
                    e,
                )
            })?;
            if q_already {
                res.skipped += 1;
                continue;
            }
            let sk = copy_tree_atomic(&q.copy.path, &qdest, &ex, opts.dry_run).map_err(|e| {
                wrap_io(
                    format!("quarantine {}/{}", d.group.repo_name, q.copy.machine),
                    e,
                )
            })?;
            res.skipped_files.extend(sk);
            res.quarantined += 1;
        }

        // 3. union remotes (Strategy B): fold every copy's history into canonical
        let mut used_remotes: HashSet<String> = HashSet::new();
        for u in &d.union_remotes {
            let mut name = u.name.clone();
            let mut i = 1;
            while used_remotes.contains(&name) {
                name = format!("{}-{}", u.name, i);
                i += 1;
            }
            used_remotes.insert(name.clone());
            if opts.dry_run {
                res.unioned += 1;
                continue;
            }
            // idempotent on re-run: ignore result
            let _ = r.run(&dest, &["remote", "remove", &name]);
            r.run(&dest, &["remote", "add", &name, &u.path])
                .map_err(|e| Error::Git(GitError {
                    args: e.args.clone(),
                    dir: e.dir.clone(),
                    cause: format!("add union remote {}: {}", name, e.cause),
                    stderr: e.stderr.clone(),
                }))?;
            let refspec = format!("+refs/heads/*:refs/remotes/{}/*", name);
            r.run(&dest, &["fetch", &name, &refspec, "--tags"])
                .map_err(|e| Error::Git(GitError {
                    args: e.args.clone(),
                    dir: e.dir.clone(),
                    cause: format!("fetch union remote {}: {}", name, e.cause),
                    stderr: e.stderr.clone(),
                }))?;
            for b in &u.branches {
                let local = format!("consolidate/{}/{}", name, b);
                let remote = format!("{}/{}", name, b);
                let _ = r.run(&dest, &["branch", "--force", &local, &remote]);
            }
            res.unioned += 1;
        }
    }
    Ok(res)
}

/// Returns a copy of `ex` with `"_quarantine"` appended. The slice is copied,
/// not mutated in place (`ex` may be reused across decisions). `_quarantine` is
/// reposmerge's own reserved output namespace nested under a canonical dest; it
/// must be excluded from the canonical idempotency hash or a nested quarantine
/// copy would permanently defeat idempotency. (See Go doc-comment for the full
/// rationale.)
fn canonical_excludes(ex: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(ex.len() + 1);
    out.extend_from_slice(ex);
    out.push("_quarantine".to_string());
    out
}

/// Reports whether `dst` already holds the same working tree as `src` (per
/// `tree_hash`, same exclude list). A nonexistent `dst` never counts as
/// "already consolidated" (`tree_hash` returns "" for a missing root).
fn already_consolidated(src: &str, dst: &str, ex: &[String]) -> Result<bool, Error> {
    let src_hash =
        tree_hash(src, ex).map_err(|e| wrap_io(format!("hash src {}", src), e))?;
    let dst_hash =
        tree_hash(dst, ex).map_err(|e| wrap_io(format!("hash dst {}", dst), e))?;
    Ok(!dst_hash.is_empty() && src_hash == dst_hash)
}

/// Wraps a nested [`Error`] with a Go-style `"<prefix>: <inner>"` message,
/// preserving the underlying variant so the error chain stays intact.
fn wrap(prefix: String, e: Error) -> Error {
    match e {
        Error::Io(io) => Error::Io(std::io::Error::new(
            io.kind(),
            format!("{}: {}", prefix, io),
        )),
        Error::Git(g) => Error::Git(GitError {
            args: g.args.clone(),
            dir: g.dir.clone(),
            cause: format!("{}: {}", prefix, g.cause),
            stderr: g.stderr.clone(),
        }),
    }
}

/// Wraps a raw `io::Error` with a Go-style `"<prefix>: <inner>"` message.
fn wrap_io(prefix: String, e: std::io::Error) -> Error {
    Error::Io(std::io::Error::new(e.kind(), format!("{}: {}", prefix, e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gitx::Fake;
    use crate::model::{Copy, Decision, Fingerprint, Group, QuarantineItem, StrategyKind, UnionRemote};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Unique temp dir per call (mirrors Go t.TempDir()).
    fn temp_dir() -> String {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let mut p = std::env::temp_dir();
        p.push(format!("reposmerge-consolidate-{}-{}", pid, n));
        fs::create_dir_all(&p).unwrap();
        p.to_str().unwrap().to_string()
    }

    fn join(base: &str, parts: &[&str]) -> String {
        let mut p = PathBuf::from(base);
        for part in parts {
            p.push(part);
        }
        p.to_str().unwrap().to_string()
    }

    fn write_file(path: &str, content: &str) {
        if let Some(parent) = std::path::Path::new(path).parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn copy_with_root(path: &str, machine: &str, root: &str) -> Copy {
        Copy {
            path: path.to_string(),
            machine: machine.to_string(),
            fp: Fingerprint {
                root_commits: vec![root.to_string()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn group(repo: &str, owner: &str) -> Group {
        Group {
            repo_name: repo.to_string(),
            owner: owner.to_string(),
            ..Default::default()
        }
    }

    // 1. TestApplyUniqueUnionRemoteNamesDryRun
    #[test]
    fn test_apply_unique_union_remote_names_dry_run() {
        let src = temp_dir();
        let dest = temp_dir();
        let dp = join(&dest, &["canonical", "personal", "loom"]);
        let p = Plan {
            dest: join(&dest, &["canonical"]),
            decisions: vec![Decision {
                strategy: StrategyKind::B,
                canonical: Copy {
                    path: src.clone(),
                    machine: "live".to_string(),
                    ..Default::default()
                },
                dest_path: dp,
                group: group("loom", "personal"),
                union_remotes: vec![
                    UnionRemote {
                        name: "consolidate-acer".to_string(),
                        path: temp_dir(),
                        ..Default::default()
                    },
                    UnionRemote {
                        name: "consolidate-acer".to_string(),
                        path: temp_dir(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let res = apply(
            &Fake::new(),
            &p,
            &Options {
                dry_run: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(res.unioned, 2, "expected 2 unioned (unique names)");
    }

    // 2. TestApplyDryRunWritesNothing
    #[test]
    fn test_apply_dry_run_writes_nothing() {
        let src = temp_dir();
        write_file(&join(&src, &["f.txt"]), "x");
        let dest = temp_dir();
        let p = Plan {
            dest: join(&dest, &["canonical"]),
            decisions: vec![Decision {
                strategy: StrategyKind::A,
                canonical: Copy {
                    path: src.clone(),
                    machine: "live".to_string(),
                    ..Default::default()
                },
                dest_path: join(&dest, &["canonical", "inovacc", "omni"]),
                group: group("omni", "inovacc"),
                ..Default::default()
            }],
            ..Default::default()
        };
        let res = apply(
            &Fake::new(),
            &p,
            &Options {
                dry_run: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(res.copied, 1, "expected 1 planned copy");
        assert!(
            !std::path::Path::new(&join(&dest, &["canonical"])).exists(),
            "dry-run must not create canonical/"
        );
    }

    // 3. TestApplyCopiesCanonical
    #[test]
    fn test_apply_copies_canonical() {
        let src = temp_dir();
        write_file(&join(&src, &["f.txt"]), "x");
        let dest = temp_dir();
        let dp = join(&dest, &["canonical", "inovacc", "omni"]);
        let p = Plan {
            dest: join(&dest, &["canonical"]),
            decisions: vec![Decision {
                strategy: StrategyKind::A,
                canonical: Copy {
                    path: src.clone(),
                    machine: "live".to_string(),
                    ..Default::default()
                },
                dest_path: dp.clone(),
                group: group("omni", "inovacc"),
                ..Default::default()
            }],
            ..Default::default()
        };
        apply(&Fake::new(), &p, &Options::default()).unwrap();
        assert!(
            std::path::Path::new(&join(&dp, &["f.txt"])).exists(),
            "canonical file not copied"
        );
    }

    // 4. TestApplyDisambiguatesDestCollision
    #[test]
    fn test_apply_disambiguates_dest_collision() {
        let src_a = temp_dir();
        write_file(&join(&src_a, &["a.txt"]), "A");
        let src_b = temp_dir();
        write_file(&join(&src_b, &["b.txt"]), "B");
        let dest = temp_dir();
        let dp = join(&dest, &["canonical", "personal", "daemon"]);
        let mk_dec = |src: &str, machine: &str, root: &str| Decision {
            strategy: StrategyKind::A,
            canonical: copy_with_root(src, machine, root),
            dest_path: dp.clone(),
            group: group("daemon", "personal"),
            ..Default::default()
        };
        let p = Plan {
            dest: join(&dest, &["canonical"]),
            decisions: vec![
                mk_dec(&src_a, "live", "rootAAAAAA"),
                mk_dec(&src_b, "acer", "rootBBBBBB"),
            ],
            ..Default::default()
        };
        apply(&Fake::new(), &p, &Options::default()).unwrap();
        assert!(
            std::path::Path::new(&join(&dp, &["a.txt"])).exists(),
            "first canonical not at base dest"
        );
        assert!(
            std::path::Path::new(&format!("{}-rootBBB", dp)).exists(),
            "second copy not disambiguated to dp-rootBBB"
        );
        assert!(
            !std::path::Path::new(&join(&dp, &["b.txt"])).exists(),
            "second copy leaked into first canonical dest"
        );
    }

    // 5. TestApplyUnionRemoteIssuesGitCommands
    #[test]
    fn test_apply_union_remote_issues_git_commands() {
        let src = temp_dir();
        write_file(&join(&src, &["f.txt"]), "x");
        let dest = temp_dir();
        let dp = join(&dest, &["canonical", "personal", "loom"]);
        let union_src = temp_dir();
        let p = Plan {
            dest: join(&dest, &["canonical"]),
            decisions: vec![Decision {
                strategy: StrategyKind::B,
                canonical: Copy {
                    path: src.clone(),
                    machine: "live".to_string(),
                    ..Default::default()
                },
                dest_path: dp,
                group: group("loom", "personal"),
                union_remotes: vec![UnionRemote {
                    name: "consolidate-acer".to_string(),
                    path: union_src.clone(),
                    branches: vec!["main".to_string(), "feature-x".to_string()],
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let fake = Fake::new();
        let res = apply(
            &fake,
            &p,
            &Options {
                dry_run: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(res.unioned, 1, "expected Unioned=1");
        let want_calls = vec![
            "remote remove consolidate-acer".to_string(),
            format!("remote add consolidate-acer {}", union_src),
            "fetch consolidate-acer +refs/heads/*:refs/remotes/consolidate-acer/* --tags"
                .to_string(),
            "branch --force consolidate/consolidate-acer/main consolidate-acer/main".to_string(),
            "branch --force consolidate/consolidate-acer/feature-x consolidate-acer/feature-x"
                .to_string(),
        ];
        assert_eq!(fake.calls(), want_calls);
    }

    // 6. TestApplyUnionRemoteAddErrorPropagates
    #[test]
    fn test_apply_union_remote_add_error_propagates() {
        let src = temp_dir();
        let dest = temp_dir();
        let dp = join(&dest, &["canonical", "personal", "loom"]);
        let union_src = temp_dir();
        let p = Plan {
            dest: join(&dest, &["canonical"]),
            decisions: vec![Decision {
                strategy: StrategyKind::B,
                canonical: Copy {
                    path: src.clone(),
                    machine: "live".to_string(),
                    ..Default::default()
                },
                dest_path: dp,
                group: group("loom", "personal"),
                union_remotes: vec![UnionRemote {
                    name: "consolidate-acer".to_string(),
                    path: union_src.clone(),
                    branches: vec!["main".to_string()],
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let fake = Fake::new().with_error(
            &format!("remote add consolidate-acer {}", union_src),
            "fake git failure",
        );
        let r = apply(
            &fake,
            &p,
            &Options {
                dry_run: false,
                ..Default::default()
            },
        );
        assert!(r.is_err(), "expected error when remote add fails");
    }

    // 7. TestApplyQuarantineCopiesFiles
    #[test]
    fn test_apply_quarantine_copies_files() {
        let canonical_src = temp_dir();
        write_file(&join(&canonical_src, &["main.go"]), "package main");
        let quarantine_src = temp_dir();
        write_file(
            &join(&quarantine_src, &["divergent.go"]),
            "package main // divergent",
        );
        let dest = temp_dir();
        let dp = join(&dest, &["canonical", "inovacc", "omni"]);
        let qdp = join(&dp, &["_quarantine", "acer"]);
        let p = Plan {
            dest: join(&dest, &["canonical"]),
            decisions: vec![Decision {
                strategy: StrategyKind::A,
                canonical: Copy {
                    path: canonical_src.clone(),
                    machine: "live".to_string(),
                    ..Default::default()
                },
                dest_path: dp.clone(),
                group: group("omni", "inovacc"),
                quarantine: vec![QuarantineItem {
                    copy: Copy {
                        path: quarantine_src.clone(),
                        machine: "acer".to_string(),
                        ..Default::default()
                    },
                    dest_path: qdp.clone(),
                    reason: "unreachable-commits".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let res = apply(
            &Fake::new(),
            &p,
            &Options {
                dry_run: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(res.quarantined, 1, "expected Quarantined=1");
        assert!(
            std::path::Path::new(&join(&qdp, &["divergent.go"])).exists(),
            "quarantine dest missing copied file"
        );
        assert!(
            std::path::Path::new(&join(&dp, &["main.go"])).exists(),
            "canonical dest missing copied file"
        );
    }

    // 8. TestApplyQuarantineDisambiguatesDestCollision
    #[test]
    fn test_apply_quarantine_disambiguates_dest_collision() {
        let canonical_src = temp_dir();
        let q_src_a = temp_dir();
        write_file(&join(&q_src_a, &["a.txt"]), "A");
        let q_src_b = temp_dir();
        write_file(&join(&q_src_b, &["b.txt"]), "B");
        let dest = temp_dir();
        let dp = join(&dest, &["canonical", "inovacc", "omni"]);
        let qdp = join(&dp, &["_quarantine", "same"]);
        let p = Plan {
            dest: join(&dest, &["canonical"]),
            decisions: vec![Decision {
                strategy: StrategyKind::A,
                canonical: Copy {
                    path: canonical_src.clone(),
                    machine: "live".to_string(),
                    ..Default::default()
                },
                dest_path: dp,
                group: group("omni", "inovacc"),
                quarantine: vec![
                    QuarantineItem {
                        copy: copy_with_root(&q_src_a, "acer", "rootAAAAAA"),
                        dest_path: qdp.clone(),
                        ..Default::default()
                    },
                    QuarantineItem {
                        copy: copy_with_root(&q_src_b, "dell", "rootBBBBBB"),
                        dest_path: qdp.clone(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let res = apply(
            &Fake::new(),
            &p,
            &Options {
                dry_run: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(res.quarantined, 2, "expected Quarantined=2");
        assert!(
            std::path::Path::new(&join(&qdp, &["a.txt"])).exists(),
            "first quarantine not at base dest"
        );
        assert!(
            std::path::Path::new(&format!("{}-rootBBB", qdp)).exists(),
            "second quarantine not disambiguated to qdp-rootBBB"
        );
    }

    // 9. TestApplySkipsAlreadyConsolidatedCanonical
    #[test]
    fn test_apply_skips_already_consolidated_canonical() {
        let src = temp_dir();
        write_file(&join(&src, &["f.txt"]), "same");
        let dest = temp_dir();
        let dp = join(&dest, &["canonical", "personal", "twice"]);
        let p = Plan {
            dest: join(&dest, &["canonical"]),
            decisions: vec![Decision {
                strategy: StrategyKind::A,
                canonical: Copy {
                    path: src.clone(),
                    machine: "live".to_string(),
                    ..Default::default()
                },
                dest_path: dp,
                group: group("twice", "personal"),
                ..Default::default()
            }],
            ..Default::default()
        };
        apply(&Fake::new(), &p, &Options::default()).unwrap();
        let res2 = apply(&Fake::new(), &p, &Options::default()).unwrap();
        assert_eq!(res2.skipped, 1, "expected Skipped=1 on second run");
        assert_eq!(res2.copied, 0, "expected Copied=0 on second run");
    }

    // 10. TestApplyIdempotentWithNestedQuarantine
    #[test]
    fn test_apply_idempotent_with_nested_quarantine() {
        let canonical_src = temp_dir();
        let quarantine_src = temp_dir();
        write_file(&join(&quarantine_src, &["q.txt"]), "same");
        let dest = temp_dir();
        let dp = join(&dest, &["canonical", "inovacc", "omni"]);
        let qdp = join(&dp, &["_quarantine", "acer"]);
        let p = Plan {
            dest: join(&dest, &["canonical"]),
            decisions: vec![Decision {
                strategy: StrategyKind::A,
                canonical: Copy {
                    path: canonical_src.clone(),
                    machine: "live".to_string(),
                    ..Default::default()
                },
                dest_path: dp,
                group: group("omni", "inovacc"),
                quarantine: vec![QuarantineItem {
                    copy: Copy {
                        path: quarantine_src.clone(),
                        machine: "acer".to_string(),
                        ..Default::default()
                    },
                    dest_path: qdp.clone(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let res1 = apply(&Fake::new(), &p, &Options::default()).unwrap();
        assert_eq!(res1.copied, 1, "first run Copied");
        assert_eq!(res1.quarantined, 1, "first run Quarantined");
        assert!(
            std::path::Path::new(&join(&qdp, &["q.txt"])).exists(),
            "quarantine file missing after first run"
        );

        let res2 = apply(&Fake::new(), &p, &Options::default()).unwrap();
        assert_eq!(res2.copied, 0, "second run: canonical skipped");
        assert_eq!(res2.quarantined, 0, "second run: quarantine skipped");
        assert_eq!(res2.skipped, 2, "second run Skipped=2");
        assert!(
            std::path::Path::new(&join(&qdp, &["q.txt"])).exists(),
            "quarantine file missing after second run"
        );
    }

    // 11. TestApplyIsIdempotentOnSecondRun — REQUIRES real `git` on PATH.
    fn git_run(dir: &str, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap_or_else(|e| panic!("git {:?}: {}", args, e));
        assert!(
            out.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn init_git_repo(dir: &str) {
        fs::create_dir_all(dir).unwrap();
        git_run(dir, &["init", "-q"]);
        git_run(dir, &["config", "user.email", "t@t"]);
        git_run(dir, &["config", "user.name", "t"]);
        git_run(dir, &["config", "commit.gpgsign", "false"]);
        write_file(&join(dir, &["f.txt"]), "x");
        git_run(dir, &["add", "."]);
        git_run(dir, &["commit", "-qm", "initial"]);
    }

    #[test]
    fn test_apply_is_idempotent_on_second_run() {
        let src = temp_dir();
        init_git_repo(&src);
        let dest = temp_dir();
        let dp = join(&dest, &["canonical", "personal", "idem"]);
        let p = Plan {
            dest: join(&dest, &["canonical"]),
            decisions: vec![Decision {
                strategy: StrategyKind::A,
                canonical: Copy {
                    path: src.clone(),
                    machine: "live".to_string(),
                    ..Default::default()
                },
                dest_path: dp.clone(),
                group: group("idem", "personal"),
                ..Default::default()
            }],
            ..Default::default()
        };
        let res1 = apply(
            &Fake::new(),
            &p,
            &Options {
                dry_run: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(res1.copied, 1, "first run Copied=1");
        assert_eq!(res1.skipped, 0, "first run Skipped=0");
        let before = fs::read(join(&dp, &["f.txt"])).unwrap();

        let res2 = apply(
            &Fake::new(),
            &p,
            &Options {
                dry_run: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(res2.skipped >= 1, "second run Skipped>=1");
        assert_eq!(res2.copied, 0, "second run Copied=0");
        let after = fs::read(join(&dp, &["f.txt"])).unwrap();
        assert_eq!(before, after, "dest contents changed on idempotent second run");
    }
}
