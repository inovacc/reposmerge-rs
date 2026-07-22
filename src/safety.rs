//! safety — faithful 1:1 Rust port of internal/safety/safety.go.
//!
//! Reachability proofs (plan-based + physical/post-apply) and resilient tree
//! copy/hash helpers used by consolidation. Ports Go `filepath.Walk` semantics
//! (skip-subtree, per-file error resilience) onto `walkdir`.

use std::fs;
use std::io::{self, Write as _};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::gitx::Runner;
use crate::model::{Plan, StrategyKind};

/// Violation is a source commit that would not survive consolidation.
///
/// Go fields Repo/Machine/SHA → snake_case per the glossary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub repo: String,
    pub machine: String,
    pub sha: String,
}

/// ReachabilityProof asserts every source commit survives in canonical or a
/// quarantined copy. Pre-apply (plan-based) gate.
pub fn reachability_proof(p: &Plan) -> Vec<Violation> {
    let mut vio = Vec::new();
    for d in &p.decisions {
        let canon: std::collections::HashSet<&str> = d
            .canonical
            .fp
            .all_commits
            .iter()
            .map(|s| s.as_str())
            .collect();
        let quarantined: std::collections::HashSet<&str> =
            d.quarantine.iter().map(|q| q.copy.path.as_str()).collect();
        // Strategy B folds all copies into canonical via union remotes -> all reachable.
        let union_all = d.strategy == StrategyKind::B;

        for c in &d.group.copies {
            if c.path == d.canonical.path {
                // canonical identity is the path
                continue;
            }
            let is_q = quarantined.contains(c.path.as_str());
            if union_all || is_q {
                continue; // physically preserved
            }
            for s in &c.fp.all_commits {
                if !canon.contains(s.as_str()) {
                    vio.push(Violation {
                        repo: d.group.repo_name.clone(),
                        machine: c.machine.clone(),
                        sha: s.clone(),
                    });
                }
            }
        }
    }
    vio
}

/// PhysicalReachability is the POST-APPLY proof for local-only (Strategy B)
/// repos: it queries the REAL consolidated repo and asserts every source commit
/// is actually present. Go `context.Context` is DROPPED (glossary).
pub fn physical_reachability(r: &dyn Runner, p: &Plan) -> Vec<Violation> {
    let mut vio = Vec::new();
    for d in &p.decisions {
        if d.strategy != StrategyKind::B {
            continue;
        }
        for c in &d.group.copies {
            for sha in &c.fp.all_commits {
                let spec = format!("{sha}^{{commit}}");
                if r.run(&d.dest_path, &["cat-file", "-e", &spec]).is_err() {
                    vio.push(Violation {
                        repo: d.group.repo_name.clone(),
                        machine: c.machine.clone(),
                        sha: sha.clone(),
                    });
                }
            }
        }
    }
    vio
}

/// CopyTree recursively copies src->dst, skipping directories named in `skip`.
/// Resilient: files it cannot read are recorded and skipped rather than
/// aborting. Returns the list of skipped (unreadable) source paths.
///
/// Faithful to Go `filepath.Walk`: a walk error for an entry appends the entry
/// path to `skipped` (and prunes the subtree for a dir); it does NOT abort.
pub fn copy_tree(src: &str, dst: &str, skip: &[String], dry_run: bool) -> io::Result<Vec<String>> {
    let skip_set: std::collections::HashSet<&str> = skip.iter().map(|s| s.as_str()).collect();
    let src_path = Path::new(src);
    let dst_path = Path::new(dst);
    let mut skipped: Vec<String> = Vec::new();

    let mut it = walkdir::WalkDir::new(src_path).into_iter();
    loop {
        let entry = match it.next() {
            None => break,
            Some(Ok(e)) => e,
            Some(Err(err)) => {
                // Go: walkErr != nil -> append path; if dir, SkipDir; else nil.
                // walkdir already will not descend into an unreadable dir, so
                // recording the path and continuing mirrors the observable
                // behavior (incl. the nonexistent-root case: one error entry
                // whose path == src, then the walk ends -> skipped=[src], Ok).
                let p = err
                    .path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| src.to_string());
                skipped.push(p);
                continue;
            }
        };

        let is_dir = entry.file_type().is_dir();
        let is_root = entry.depth() == 0; // rel == "."
        let rel = entry
            .path()
            .strip_prefix(src_path)
            .unwrap_or_else(|_| Path::new(""));

        if is_dir {
            let name = entry.file_name().to_string_lossy();
            if skip_set.contains(name.as_ref()) && !is_root {
                it.skip_current_dir();
                continue;
            }
            if dry_run {
                continue;
            }
            let target = dst_path.join(rel);
            if fs::create_dir_all(&target).is_err() {
                skipped.push(entry.path().to_string_lossy().into_owned());
                it.skip_current_dir();
            }
            continue;
        }

        // file
        if dry_run {
            continue;
        }
        let mode = file_mode(&entry.metadata().ok());
        let target = dst_path.join(rel);
        if copy_file(entry.path(), &target, mode).is_err() {
            skipped.push(entry.path().to_string_lossy().into_owned());
        }
    }

    Ok(skipped)
}

/// CopyTreeAtomic copies src->dst atomically from dst's point of view: it copies
/// into a temporary sibling (`dst + ".reposmerge-tmp"`) and only renames it into
/// place once the whole tree is copied. On any error the temp dir is removed so
/// no partial directory is ever left at dst. In dry-run it behaves like CopyTree.
pub fn copy_tree_atomic(
    src: &str,
    dst: &str,
    skip: &[String],
    dry_run: bool,
) -> io::Result<Vec<String>> {
    if dry_run {
        return copy_tree(src, dst, skip, true);
    }

    if let Some(parent) = Path::new(dst).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = fs::create_dir_all(parent) {
                return Err(io::Error::new(
                    e.kind(),
                    format!("prepare dest parent for {dst}: {e}"),
                ));
            }
        }
    }

    let tmp = format!("{dst}.reposmerge-tmp");
    if let Err(e) = remove_all(Path::new(&tmp)) {
        return Err(io::Error::new(
            e.kind(),
            format!("clear stale temp dir {tmp}: {e}"),
        ));
    }

    let skipped = match copy_tree(src, &tmp, skip, false) {
        Ok(s) => s,
        Err(e) => {
            let _ = remove_all(Path::new(&tmp)); // best-effort rollback
            return Err(e);
        }
    };

    if let Err(e) = remove_all(Path::new(dst)) {
        let _ = remove_all(Path::new(&tmp));
        return Err(io::Error::new(
            e.kind(),
            format!("clear existing dest {dst}: {e}"),
        ));
    }

    if let Err(e) = fs::rename(&tmp, dst) {
        let _ = remove_all(Path::new(&tmp));
        return Err(io::Error::new(
            e.kind(),
            format!("rename {tmp} to {dst}: {e}"),
        ));
    }

    Ok(skipped)
}

/// TreeHash returns a stable hex sha256 over the working tree at `root`: the
/// relpath, mode bits, and content-sha256 of every file, in sorted relpath
/// order, fed into one running hash. Always skips ".git" plus any dir named in
/// `skip`. A missing root returns ("", no error). Unreadable files are skipped.
pub fn tree_hash(root: &str, skip: &[String]) -> io::Result<String> {
    let root_path = Path::new(root);
    match fs::metadata(root_path) {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(io::Error::new(e.kind(), format!("stat {root}: {e}"))),
    }

    let mut skip_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    skip_set.insert(".git".to_string());
    for s in skip {
        skip_set.insert(s.clone());
    }

    struct FileEntry {
        rel: String,
        mode: u32,
        path: std::path::PathBuf,
    }
    let mut entries: Vec<FileEntry> = Vec::new();

    let mut it = walkdir::WalkDir::new(root_path).into_iter();
    loop {
        let entry = match it.next() {
            None => break,
            Some(Ok(e)) => e,
            Some(Err(_)) => continue, // dir walkErr -> skip; walkdir won't descend
        };
        let is_dir = entry.file_type().is_dir();
        let is_root = entry.depth() == 0;
        if is_dir {
            let name = entry.file_name().to_string_lossy();
            if skip_set.contains(name.as_ref()) && !is_root {
                it.skip_current_dir();
            }
            continue;
        }
        let rel_os = entry
            .path()
            .strip_prefix(root_path)
            .unwrap_or_else(|_| Path::new(""));
        let rel = to_slash(&rel_os.to_string_lossy());
        let mode = file_mode(&entry.metadata().ok());
        entries.push(FileEntry {
            rel,
            mode,
            path: entry.path().to_path_buf(),
        });
    }

    // sort by rel (byte order == Go sort.Slice with rel[i] < rel[j])
    entries.sort_by(|a, b| a.rel.cmp(&b.rel));

    let mut h = Sha256::new();
    for e in &entries {
        let sum = match file_sha256(&e.path) {
            Ok(s) => s,
            Err(_) => continue, // unreadable file: skip, mirrors CopyTree
        };
        // EXACT Go framing: "path:%s\x00mode:%o\x00sha256:%x\x00"
        let _ = write!(
            h,
            "path:{}\0mode:{:o}\0sha256:{}\0",
            e.rel,
            e.mode,
            hex::encode(sum)
        );
    }
    Ok(hex::encode(h.finalize()))
}

fn file_sha256(path: &Path) -> io::Result<[u8; 32]> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    io::copy(&mut f, &mut h)?;
    let out = h.finalize();
    let mut sum = [0u8; 32];
    sum.copy_from_slice(&out);
    Ok(sum)
}

fn copy_file(src: &Path, dst: &Path, mode: u32) -> io::Result<()> {
    let mut in_f = fs::File::open(src)?;
    if let Some(parent) = dst.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    // O_CREATE|O_WRONLY|O_TRUNC with mode
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode);
    }
    #[cfg(not(unix))]
    {
        let _ = mode; // Windows: mode largely ignored (Go behaves likewise)
    }
    let mut out_f = opts.open(dst)?;
    if let Err(e) = io::copy(&mut in_f, &mut out_f) {
        drop(out_f);
        let _ = fs::remove_file(dst); // don't leave a partial file behind
        return Err(e);
    }
    Ok(())
}

/// Mirror Go `os.RemoveAll`: absent path -> Ok(()); other failures -> Err.
fn remove_all(path: &Path) -> io::Result<()> {
    match fs::metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        _ => {}
    }
    let ft = match fs::symlink_metadata(path) {
        Ok(m) => m.file_type(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if ft.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// Portable, deterministic per-file mode (glossary MODE PARITY scheme).
///
/// - unix: full st_mode-ish via permissions().mode() (like Go incl. bits).
/// - windows: synthesize a Go-like mode: 0o444 if readonly else 0o666.
///
/// Cross-language mode bytes are NOT required to match Go; only same-platform
/// determinism (identical trees hash equal).
fn file_mode(meta: &Option<fs::Metadata>) -> u32 {
    match meta {
        None => 0,
        Some(m) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                m.permissions().mode()
            }
            #[cfg(not(unix))]
            {
                if m.permissions().readonly() {
                    0o444
                } else {
                    0o666
                }
            }
        }
    }
}

fn to_slash(s: &str) -> String {
    if std::path::MAIN_SEPARATOR == '/' {
        s.to_string()
    } else {
        s.replace(std::path::MAIN_SEPARATOR, "/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gitx::Fake;
    use crate::model::{Copy, Decision, Fingerprint, Group, Plan, QuarantineItem, StrategyKind};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn unique_dir(tag: &str) -> std::path::PathBuf {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let p = std::env::temp_dir().join(format!("reposmerge-safety-{tag}-{pid}-{n}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn copy_with(path: &str, machine: &str, commits: &[&str]) -> Copy {
        Copy {
            path: path.to_string(),
            machine: machine.to_string(),
            fp: Fingerprint {
                all_commits: commits.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn reachability_proof_passes() {
        let canon = copy_with("/live", "live", &["a", "b", "c"]);
        let other = copy_with("/acer", "acer", &["a", "b"]);
        let p = Plan {
            decisions: vec![Decision {
                group: Group {
                    repo_name: "x".to_string(),
                    copies: vec![canon.clone(), other],
                    ..Default::default()
                },
                canonical: canon,
                ..Default::default()
            }],
            ..Default::default()
        };
        let v = reachability_proof(&p);
        assert!(v.is_empty(), "expected no violations, got {v:?}");
    }

    #[test]
    fn reachability_proof_catches_loss() {
        let canon = copy_with("/live", "live", &["a"]);
        let other = copy_with("/acer", "acer", &["a", "z"]);
        let p = Plan {
            decisions: vec![Decision {
                group: Group {
                    repo_name: "x".to_string(),
                    copies: vec![canon.clone(), other],
                    ..Default::default()
                },
                canonical: canon,
                ..Default::default()
            }],
            ..Default::default()
        };
        let v = reachability_proof(&p);
        assert!(
            v.len() == 1 && v[0].sha == "z",
            "expected violation for z, got {v:?}"
        );
    }

    #[test]
    fn copy_tree_skips_excluded() {
        let src = unique_dir("cts-src");
        fs::create_dir_all(src.join("node_modules").join("x")).unwrap();
        write_file(&src.join("main.go"), "package main");
        write_file(&src.join("node_modules").join("x").join("junk"), "j");
        let dst = unique_dir("cts-dstparent").join("out");

        copy_tree(
            src.to_str().unwrap(),
            dst.to_str().unwrap(),
            &["node_modules".to_string()],
            false,
        )
        .unwrap();

        assert!(dst.join("main.go").exists(), "main.go not copied");
        assert!(
            !dst.join("node_modules").exists(),
            "node_modules should have been skipped"
        );
    }

    #[test]
    fn reachability_proof_quarantine_preserves() {
        let canon = copy_with("/live", "", &["a"]);
        let other = copy_with("/acer", "acer", &["a", "z"]);
        let p = Plan {
            decisions: vec![Decision {
                strategy: StrategyKind::A,
                group: Group {
                    repo_name: "x".to_string(),
                    copies: vec![canon.clone(), other.clone()],
                    ..Default::default()
                },
                canonical: canon,
                quarantine: vec![QuarantineItem {
                    copy: other,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let v = reachability_proof(&p);
        assert!(
            v.is_empty(),
            "quarantined copy should preserve 'z'; got {v:?}"
        );
    }

    #[test]
    fn copy_tree_atomic_success_no_leftover_temp() {
        let src = unique_dir("ctas-src");
        write_file(&src.join("main.go"), "package main");
        fs::create_dir_all(src.join("sub")).unwrap();
        write_file(&src.join("sub").join("f.txt"), "f");
        let dst = unique_dir("ctas-dstparent").join("out");

        copy_tree_atomic(src.to_str().unwrap(), dst.to_str().unwrap(), &[], false).unwrap();

        assert!(dst.join("main.go").exists(), "main.go not copied");
        assert!(
            dst.join("sub").join("f.txt").exists(),
            "sub/f.txt not copied"
        );
        let tmp = format!("{}.reposmerge-tmp", dst.to_str().unwrap());
        assert!(
            !Path::new(&tmp).exists(),
            "leftover temp dir sibling should not exist"
        );
    }

    #[test]
    fn copy_tree_atomic_nonexistent_src_errors() {
        let src = unique_dir("ctane-srcparent").join("does-not-exist");
        let dst = unique_dir("ctane-dstparent").join("out");

        let res = copy_tree_atomic(src.to_str().unwrap(), dst.to_str().unwrap(), &[], false);
        assert!(res.is_err(), "expected error for nonexistent src");
        assert!(!dst.exists(), "dst should not exist after failed copy");
        let tmp = format!("{}.reposmerge-tmp", dst.to_str().unwrap());
        assert!(
            !Path::new(&tmp).exists(),
            "temp dir should have been removed after failed copy"
        );
    }

    #[test]
    fn copy_tree_atomic_rollback_on_failure() {
        // WINDOWS-ONLY: relies on Windows delete-sharing semantics to force
        // remove_dir_all(dst) to fail (Go's runtime.GOOS != "windows" skip).
        if !cfg!(windows) {
            return;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;

            let src = unique_dir("ctar-src");
            write_file(&src.join("newfile.txt"), "from-src");

            let dst = unique_dir("ctar-dstparent").join("out");
            fs::create_dir_all(&dst).unwrap();
            let locked_path = dst.join("locked.txt");
            write_file(&locked_path, "original");

            // Open a handle on locked.txt that PREVENTS deletion:
            // FILE_SHARE_READ|FILE_SHARE_WRITE (0x1|0x2=3), NO FILE_SHARE_DELETE.
            let handle = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(3)
                .open(&locked_path)
                .unwrap();

            let tmp = format!("{}.reposmerge-tmp", dst.to_str().unwrap());

            let res = copy_tree_atomic(src.to_str().unwrap(), dst.to_str().unwrap(), &[], false);
            assert!(
                res.is_err(),
                "expected CopyTreeAtomic to fail when dst cannot be cleared"
            );

            assert!(
                !Path::new(&tmp).exists(),
                "temp sibling should have been removed by rollback after populated copy"
            );
            assert!(dst.exists(), "dst should still exist after failed swap");
            assert!(
                !dst.join("newfile.txt").exists(),
                "dst should not contain newly-copied src files: no partial swap"
            );
            assert!(
                locked_path.exists(),
                "dst's original file should be untouched"
            );

            drop(handle); // keep handle alive until after assertions
        }
    }

    #[test]
    fn copy_tree_atomic_dry_run() {
        let src = unique_dir("ctadr-src");
        write_file(&src.join("main.go"), "package main");
        let dst = unique_dir("ctadr-dstparent").join("out");

        let actions =
            copy_tree_atomic(src.to_str().unwrap(), dst.to_str().unwrap(), &[], true).unwrap();
        assert!(
            actions.is_empty(),
            "dry-run should report no skipped files, got {actions:?}"
        );
        assert!(!dst.exists(), "dry-run should not create dst");
        let tmp = format!("{}.reposmerge-tmp", dst.to_str().unwrap());
        assert!(
            !Path::new(&tmp).exists(),
            "dry-run should not create temp dir"
        );
    }

    #[test]
    fn tree_hash_stable_across_calls() {
        let dir = unique_dir("ths");
        write_file(&dir.join("a.txt"), "hello");
        fs::create_dir_all(dir.join("sub")).unwrap();
        write_file(&dir.join("sub").join("b.txt"), "world");

        let h1 = tree_hash(dir.to_str().unwrap(), &[]).unwrap();
        let h2 = tree_hash(dir.to_str().unwrap(), &[]).unwrap();
        assert!(
            !h1.is_empty() && h1 == h2,
            "expected stable non-empty hash, got {h1:?} vs {h2:?}"
        );
    }

    #[test]
    fn tree_hash_equal_for_identical_trees_different_paths() {
        let dir_a = unique_dir("theq-a");
        write_file(&dir_a.join("a.txt"), "hello");
        let dir_b = unique_dir("theq-b");
        write_file(&dir_b.join("a.txt"), "hello");

        let ha = tree_hash(dir_a.to_str().unwrap(), &[]).unwrap();
        let hb = tree_hash(dir_b.to_str().unwrap(), &[]).unwrap();
        assert!(
            !ha.is_empty() && ha == hb,
            "expected equal hashes, got {ha:?} vs {hb:?}"
        );
    }

    #[test]
    fn tree_hash_differs_on_content_change() {
        let dir = unique_dir("thd");
        write_file(&dir.join("a.txt"), "hello");
        let h1 = tree_hash(dir.to_str().unwrap(), &[]).unwrap();
        write_file(&dir.join("a.txt"), "goodbye");
        let h2 = tree_hash(dir.to_str().unwrap(), &[]).unwrap();
        assert!(h1 != h2, "expected different hash after content change");
    }

    #[test]
    fn tree_hash_unaffected_by_git_and_excluded_dir() {
        let dir_a = unique_dir("thu-a");
        write_file(&dir_a.join("a.txt"), "hello");
        write_file(&dir_a.join(".git").join("HEAD"), "ref: refs/heads/main");
        write_file(&dir_a.join("node_modules").join("junk.js"), "junk");

        let dir_b = unique_dir("thu-b");
        write_file(&dir_b.join("a.txt"), "hello");
        write_file(
            &dir_b.join(".git").join("HEAD"),
            "ref: refs/heads/DIFFERENT",
        );
        write_file(
            &dir_b.join("node_modules").join("junk.js"),
            "totally different",
        );

        let ha = tree_hash(dir_a.to_str().unwrap(), &["node_modules".to_string()]).unwrap();
        let hb = tree_hash(dir_b.to_str().unwrap(), &["node_modules".to_string()]).unwrap();
        assert!(
            !ha.is_empty() && ha == hb,
            "expected .git/node_modules divergence to not affect hash, got {ha:?} vs {hb:?}"
        );
    }

    #[test]
    fn tree_hash_missing_root_returns_empty_no_error() {
        let missing = unique_dir("thm").join("does-not-exist");
        let h = tree_hash(missing.to_str().unwrap(), &[]).unwrap();
        assert_eq!(h, "", "expected empty hash for missing root, got {h:?}");
    }

    #[test]
    fn physical_reachability_catches_missing() {
        let f = Fake::new()
            .with_response("cat-file -e a^{commit}", "")
            .with_error("cat-file -e z^{commit}", "not found");
        let canon = copy_with("/live", "", &["a"]);
        let other = copy_with("/acer", "acer", &["a", "z"]);
        let p = Plan {
            decisions: vec![Decision {
                strategy: StrategyKind::B,
                dest_path: "/dest".to_string(),
                group: Group {
                    repo_name: "x".to_string(),
                    copies: vec![canon.clone(), other],
                    ..Default::default()
                },
                canonical: canon,
                ..Default::default()
            }],
            ..Default::default()
        };
        let v = physical_reachability(&f, &p);
        assert!(
            v.len() == 1 && v[0].sha == "z",
            "expected physical violation for z, got {v:?}"
        );
    }
}
