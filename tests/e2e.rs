//! End-to-end integration test — faithful port of
//! `internal/e2e/e2e_test.go` (`TestConsolidatePreservesAllCommits`).
//!
//! Exercises the full library pipeline through the public `reposmerge::` API:
//! discover -> fingerprint -> group -> strategy -> reachability proof -> apply,
//! then proves via real `git log --all` that the consolidated repo contains the
//! union of every source commit, sources untouched.
//!
//! Requires a real `git` on PATH (the Go test gated this under `-short`; here
//! the conductor runs it with git available).

use std::path::{Path, PathBuf};
use std::process::Command;

use reposmerge::consolidate::{self, Options};
use reposmerge::discover::{default_scope, discover};
use reposmerge::fingerprint;
use reposmerge::gitx::new_runner;
use reposmerge::group;
use reposmerge::model::{Plan, StrategyKind};
use reposmerge::safety::{self, physical_reachability, reachability_proof};
use reposmerge::strategy;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn git");
    if !out.status.success() {
        panic!(
            "git {:?}: status {}\n{}{}",
            args,
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn mkdir(d: &Path) -> PathBuf {
    std::fs::create_dir_all(d).expect("mkdir");
    d.to_path_buf()
}

fn write(dir: &Path, name: &str, content: &str) {
    std::fs::write(dir.join(name), content).expect("write file");
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@t"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}

/// Unique temp dir under the OS temp (parity with Go `t.TempDir()`).
fn temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let d = std::env::temp_dir().join(format!("reposmerge-e2e-{tag}-{nanos}"));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn consolidate_preserves_all_commits() {
    let roots = temp_dir("roots");

    // base local-only repo with a shared root commit
    let base = mkdir(
        &roots
            .join("development")
            .join("personal")
            .join("projects")
            .join("auditor"),
    );
    init_repo(&base);
    write(&base, "shared.txt", "shared");
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "root"]);

    // replicate it (filesystem copy preserving .git) so the second copy shares lineage
    let other = roots
        .join("New folder")
        .join("acer")
        .join("projects")
        .join("auditor");
    safety::copy_tree(base.to_str().unwrap(), other.to_str().unwrap(), &[], false)
        .expect("copy_tree");

    // diverge: each copy gets a unique commit
    write(&base, "live.txt", "live");
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "live-only"]);
    write(&other, "acer.txt", "acer");
    git(&other, &["add", "."]);
    git(&other, &["commit", "-qm", "acer-only"]);

    // pipeline: discover -> fingerprint -> group -> strategy -> proof -> apply
    let (mut in_scope, _third) = discover(
        &[roots.to_str().unwrap().to_string()],
        &default_scope(),
        &consolidate::default_excludes(),
        false,
    )
    .expect("discover");
    assert_eq!(in_scope.len(), 2, "expected 2 copies discovered");

    let r = new_runner();
    for c in in_scope.iter_mut() {
        fingerprint::compute(&r, c).expect("fingerprint");
    }
    let groups = group::build(in_scope);
    assert_eq!(groups.len(), 1, "expected 1 group (shared lineage)");

    let dest = temp_dir("dest").join("canonical");
    let mut plan = Plan {
        roots: vec![roots.to_str().unwrap().to_string()],
        dest: dest.to_str().unwrap().to_string(),
        ..Default::default()
    };
    for g in &groups {
        plan.decisions
            .push(strategy::decide(g, dest.to_str().unwrap()));
    }
    assert_eq!(
        plan.decisions[0].strategy,
        StrategyKind::B,
        "expected Strategy B for local-only repo"
    );

    let vio = reachability_proof(&plan);
    assert!(
        vio.is_empty(),
        "reachability violations before apply: {vio:?}"
    );

    consolidate::apply(
        &r,
        &plan,
        &Options {
            dest: dest.to_str().unwrap().to_string(),
            ..Default::default()
        },
    )
    .expect("apply");

    // canonical must now contain ALL commits (union of both copies)
    let dest_path = &plan.decisions[0].dest_path;
    let out = Command::new("git")
        .args(["-C", dest_path, "log", "--all", "--format=%s"])
        .output()
        .expect("git log");
    let log = String::from_utf8_lossy(&out.stdout);
    for want in ["root", "live-only", "acer-only"] {
        assert!(
            log.contains(want),
            "commit {want:?} missing from consolidated repo:\n{log}"
        );
    }

    // PhysicalReachability must pass post-apply
    let pv = physical_reachability(&r, &plan);
    assert!(
        pv.is_empty(),
        "physical reachability violations after apply: {pv:?}"
    );

    // sources must be untouched
    for src in [&base, &other] {
        assert!(
            src.join(".git").exists(),
            "source {} was damaged",
            src.display()
        );
    }
}
