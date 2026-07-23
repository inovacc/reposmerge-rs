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
