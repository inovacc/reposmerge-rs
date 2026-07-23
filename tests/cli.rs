//! CLI integration tests — drive the real `reposmerge` binary end-to-end.
//!
//! Uses `assert_cmd` to run the compiled binary and shells out to real `git`
//! (via std::process::Command) to build the source trees, mirroring
//! `tests/e2e.rs`. These exercise the four subcommands' observable CLI surface
//! (stdout markers, exit status) as the parity contract. Requires `git` on PATH;
//! the conductor runs these with git available.

use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::Command as AssertCommand;
use predicates::prelude::*;
use predicates::str::contains;

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

fn init_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@t"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}

fn write(dir: &Path, name: &str, content: &str) {
    std::fs::write(dir.join(name), content).expect("write file");
}

/// Unique temp dir under the OS temp dir.
fn temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let d = std::env::temp_dir().join(format!(
        "reposmerge-cli-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Recursively copy a directory tree (preserving `.git`) — Strategy B setup.
fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let ft = entry.file_type().unwrap();
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).unwrap();
        }
    }
}

/// Build a tree containing a remote-backed repo (Strategy A) and a shared-lineage
/// local-only pair (Strategy B). Returns the tree root.
fn build_tree() -> PathBuf {
    let tree = temp_dir("tree");

    // Strategy A: a repo WITH a remote.
    let remote_repo = tree
        .join("development")
        .join("personal")
        .join("projects")
        .join("omni");
    init_repo(&remote_repo);
    write(&remote_repo, "file.txt", "hello");
    git(&remote_repo, &["add", "."]);
    git(&remote_repo, &["commit", "-qm", "init"]);
    git(
        &remote_repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/inovacc/omni.git",
        ],
    );

    // Strategy B: a shared-lineage local-only pair.
    let base = tree
        .join("development")
        .join("personal")
        .join("projects")
        .join("auditor");
    init_repo(&base);
    write(&base, "shared.txt", "shared");
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "root"]);

    // Filesystem-copy (preserving .git) to a sibling machine path.
    let other = tree
        .join("New folder")
        .join("acer")
        .join("projects")
        .join("auditor");
    copy_dir(&base, &other);

    // Diverge: each gets a unique commit.
    write(&base, "live.txt", "live");
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "live-only"]);
    write(&other, "acer.txt", "acer");
    git(&other, &["add", "."]);
    git(&other, &["commit", "-qm", "acer-only"]);

    tree
}

fn bin() -> AssertCommand {
    AssertCommand::cargo_bin("reposmerge").expect("cargo_bin reposmerge")
}

#[test]
fn scan_plan_verify_pipeline() {
    let tree = build_tree();
    let out = temp_dir("out");

    bin()
        .args([
            "scan",
            "--roots",
            tree.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--dest",
            "./canonical",
        ])
        .assert()
        .success()
        .stdout(contains("scanned:"));

    bin()
        .args([
            "plan",
            "--out",
            out.to_str().unwrap(),
            "--dest",
            "./canonical",
        ])
        .assert()
        .success()
        .stdout(contains("planned"));

    let plan_json = out.join("reports").join("plan.json");
    let inventory = out.join("reports").join("inventory.csv");
    assert!(plan_json.exists(), "plan.json missing");
    assert!(inventory.exists(), "inventory.csv missing");

    bin()
        .args(["verify", "--plan", plan_json.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("verify OK"));

    let _ = std::fs::remove_dir_all(&tree);
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn apply_dry_run_then_confirm() {
    let tree = build_tree();
    let out = temp_dir("out");
    let destdir = temp_dir("dest").join("canonical");

    bin()
        .args([
            "scan",
            "--roots",
            tree.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--dest",
            destdir.to_str().unwrap(),
        ])
        .assert()
        .success();

    bin()
        .args([
            "plan",
            "--out",
            out.to_str().unwrap(),
            "--dest",
            destdir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let plan_json = out.join("reports").join("plan.json");

    // Dry-run: no --confirm.
    bin()
        .args([
            "apply",
            "--plan",
            plan_json.to_str().unwrap(),
            "--dest",
            destdir.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("DRY-RUN"));

    // Confirm: writes the canonical tree.
    bin()
        .args([
            "apply",
            "--plan",
            plan_json.to_str().unwrap(),
            "--dest",
            destdir.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--confirm",
        ])
        .assert()
        .success()
        .stdout(contains("copied="));

    assert!(
        destdir.exists(),
        "canonical dest dir should exist after apply --confirm"
    );

    let _ = std::fs::remove_dir_all(&tree);
    let _ = std::fs::remove_dir_all(&out);
    let _ = std::fs::remove_dir_all(destdir.parent().unwrap());
}

/// Build a shared-lineage union fixture: one origin repo with real history,
/// two clones on distinct "machine" paths, and an extra local-only branch +
/// commit on the second clone. Returns `(tree, extra_sha)` where `extra_sha`
/// is the commit that only union can preserve. This is the "does it actually
/// work" case — Strategy B union that ends in a clean physical verify.
fn build_union_tree() -> (PathBuf, String) {
    let tree = temp_dir("union");
    let origin = tree.join("_origin").join("projX");
    init_repo(&origin);
    write(&origin, "README.md", "X");
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-qm", "init"]);
    write(&origin, "main.rs", "fn main() {}");
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-qm", "c2"]);

    let live = tree.join("live").join("projX");
    let dell = tree.join("dell").join("projX");
    git(
        &tree,
        &[
            "clone",
            "-q",
            origin.to_str().unwrap(),
            live.to_str().unwrap(),
        ],
    );
    git(
        &tree,
        &[
            "clone",
            "-q",
            origin.to_str().unwrap(),
            dell.to_str().unwrap(),
        ],
    );

    // Clones do NOT inherit user identity, and CI runners have no global git
    // config — configure each clone before committing into it. Also point origin
    // at an in-scope owner (`inovacc`): a clone's default origin is the local
    // `_origin` path, whose parsed owner is out-of-scope and gets classified
    // third-party (and excluded) on POSIX — where the `/`-path splits into
    // owner/repo — while on Windows the backslash path doesn't split, so it read
    // as a local-only in-scope repo. Setting an in-scope remote makes discovery
    // deterministic across platforms.
    for c in [&live, &dell] {
        git(c, &["config", "user.email", "t@t"]);
        git(c, &["config", "user.name", "t"]);
        git(c, &["config", "commit.gpgsign", "false"]);
        git(
            c,
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/inovacc/projX.git",
            ],
        );
    }

    // dell copy gains an extra local-only branch + commit; only union preserves it.
    git(&dell, &["checkout", "-q", "-b", "feature"]);
    write(&dell, "feat.txt", "f");
    git(&dell, &["add", "."]);
    git(&dell, &["commit", "-qm", "feat"]);
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&dell)
        .output()
        .expect("git rev-parse");
    let extra_sha = String::from_utf8_lossy(&out.stdout).trim().to_string();

    std::fs::remove_dir_all(tree.join("_origin")).unwrap();
    (tree, extra_sha)
}

/// The full pipeline must physically consolidate and self-prove no loss:
/// scan -> plan -> apply --confirm (physical verify OK) -> verify --physical
/// (verify OK), and the union must preserve the extra local-only commit.
#[test]
fn full_pipeline_physical_verify_preserves_branches() {
    let (tree, extra_sha) = build_union_tree();
    let out = temp_dir("out");
    let destdir = temp_dir("dest").join("canonical");

    bin()
        .args([
            "scan",
            "--roots",
            tree.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--dest",
            destdir.to_str().unwrap(),
            "--workers",
            "1",
        ])
        .assert()
        .success();

    bin()
        .args([
            "plan",
            "--out",
            out.to_str().unwrap(),
            "--dest",
            destdir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let plan_json = out.join("reports").join("plan.json");

    // apply --confirm runs a post-apply physical verification that must pass.
    bin()
        .args([
            "apply",
            "--plan",
            plan_json.to_str().unwrap(),
            "--dest",
            destdir.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--confirm",
        ])
        .assert()
        .success()
        .stdout(contains("post-apply physical verification OK"));

    // Independent physical verify pass over the real consolidated repos.
    bin()
        .args([
            "verify",
            "--plan",
            plan_json.to_str().unwrap(),
            "--physical",
        ])
        .assert()
        .success()
        .stdout(contains("verify OK"));

    // The extra local-only commit must be preserved on disk somewhere under the
    // consolidated tree. Ask with `git cat-file -e <sha>^{commit}` — the same
    // object-existence primitive the tool's own physical_reachability uses —
    // rather than `git log --all`, which only surfaces *ref-reachable* commits
    // and so is sensitive to how a given git version/config lays out refs after
    // a union fetch or quarantine copy.
    let mut found = false;
    for entry in walk_git_repos(&destdir) {
        let ok = Command::new("git")
            .args([
                "-C",
                entry.to_str().unwrap(),
                "cat-file",
                "-e",
                &format!("{extra_sha}^{{commit}}"),
            ])
            .output()
            .expect("git cat-file")
            .status
            .success();
        if ok {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "extra local-only commit {extra_sha} not preserved in consolidated tree (repos scanned: {:?})",
        walk_git_repos(&destdir)
    );

    let _ = std::fs::remove_dir_all(&tree);
    let _ = std::fs::remove_dir_all(&out);
    let _ = std::fs::remove_dir_all(destdir.parent().unwrap());
}

/// Yield every git working tree (dir containing `.git`) under `root`.
fn walk_git_repos(root: &Path) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if dir.join(".git").exists() {
            repos.push(dir.clone());
        }
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) && e.file_name() != ".git" {
                    stack.push(e.path());
                }
            }
        }
    }
    repos
}

#[test]
fn no_subcommand_shows_error() {
    bin().assert().failure();
}

#[test]
fn version_flag() {
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("reposmerge").or(contains("1.")));
}
