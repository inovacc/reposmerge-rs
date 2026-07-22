//! discover — faithful 1:1 port of Go package `discover` (internal/discover).
//!
//! Walks roots, finds git repos, classifies them in-scope vs third-party, and
//! derives machine/source labels. Read-only.
//!
//! Design decisions (recorded in PORT-GLOSSARY / PORT-TRACK):
//! - `Discover` drops Go's `context.Context` param (no cancellation tested),
//!   consistent with `gitx::Runner`.
//! - Recursive walk uses the `walkdir` crate: `std::fs` has no walker with
//!   skip-descend (prune) control equivalent to Go `filepath.WalkDir` +
//!   `filepath.SkipDir`. The loop + `it.skip_current_dir()` form mirrors
//!   `SkipDir` faithfully.
//! - `source_disc` uses `sha2` (Sha256) + `hex` — std has no crypto and the Go
//!   token is defined as the first 6 hex chars of a sha256 digest.

use std::path::{Path, MAIN_SEPARATOR};

use hex;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::gitx::{self, GitError, Runner};
use crate::model::Copy;

/// Scope decides which discovered repos are in-scope vs third-party.
#[derive(Debug, Clone)]
pub struct Scope {
    /// Matched against parsed owner.
    pub in_scope_owners: Vec<String>,
    /// Path substrings that mark re-cloneable upstreams.
    pub third_party_dirs: Vec<String>,
}

/// DefaultScope reflects the consolidation design.
pub fn default_scope() -> Scope {
    Scope {
        in_scope_owners: vec![
            "london-bridge".to_string(),
            "lb-conn".to_string(),
            "lb-common".to_string(),
            "inovacc".to_string(),
            "dyammarcano".to_string(),
        ],
        // Go: string(filepath.Separator) + "public_repos" + string(filepath.Separator)
        third_party_dirs: vec![format!("{sep}public_repos{sep}", sep = MAIN_SEPARATOR)],
    }
}

/// NormalizeURL canonicalizes a git remote URL to host/owner/repo (no scheme,
/// no .git). Faithful byte-for-byte port of the Go trimming sequence.
pub fn normalize_url(u: &str) -> String {
    let mut u = u.trim().to_string();
    if u.is_empty() {
        return String::new();
    }
    // Go strings.TrimSuffix / TrimPrefix each strip once.
    u = trim_suffix(&u, ".git");
    u = trim_prefix(&u, "ssh://");
    u = trim_prefix(&u, "https://");
    u = trim_prefix(&u, "http://");
    u = trim_prefix(&u, "git@");
    // git@github.com:owner/repo -> github.com/owner/repo
    // Go strings.Replace(u, ":", "/", 1) — replace FIRST ':' only.
    if let Some(i) = u.find(':') {
        u.replace_range(i..i + 1, "/");
    }
    // collapse any user@ that survived:
    // Go: if i := strings.Index(u, "@"); i >= 0 && i < strings.Index(u+"/", "/") { u = u[i+1:] }
    if let Some(at) = u.find('@') {
        // Index(u+"/", "/") — first '/' in u, or len(u) if none (the appended
        // '/' guarantees a match, at position len(u)).
        let slash = u.find('/').unwrap_or(u.len());
        if at < slash {
            u = u[at + 1..].to_string();
        }
    }
    trim_suffix(&u, "/")
}

/// Strip suffix once (Go strings.TrimSuffix).
fn trim_suffix(s: &str, suffix: &str) -> String {
    s.strip_suffix(suffix).unwrap_or(s).to_string()
}

/// Strip prefix once (Go strings.TrimPrefix).
fn trim_prefix(s: &str, prefix: &str) -> String {
    s.strip_prefix(prefix).unwrap_or(s).to_string()
}

/// ParseOwnerRepo extracts owner/repo from a normalized URL, falling back to
/// basename.
pub fn parse_owner_repo(norm_url: &str, basename: &str) -> (String, String) {
    if norm_url.is_empty() {
        return (String::new(), basename.to_string());
    }
    let parts: Vec<&str> = norm_url.split('/').collect();
    if parts.len() >= 3 {
        return (
            parts[parts.len() - 2].to_string(),
            parts[parts.len() - 1].to_string(),
        );
    }
    (String::new(), basename.to_string())
}

/// InferMachine derives a source label from the path.
///
/// Go does `strings.ToLower(filepath.ToSlash(path))` — ToSlash FIRST, then
/// lowercase. The switch order is significant and preserved verbatim.
pub fn infer_machine(path: &str) -> String {
    let p = to_slash(path).to_lowercase();
    if p.contains("/development/personal") || p.contains("/development/corporate") {
        // corporate/dell is a machine subdir:
        if p.contains("/corporate/dell/") {
            return "dell".to_string();
        }
        "live".to_string()
    } else if p.contains("/acer/") {
        "acer".to_string()
    } else if p.contains("/dell/") {
        "dell".to_string()
    } else if p.contains("/my drive") {
        "drive".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Go filepath.ToSlash: replace the OS separator with '/'. On Windows that
/// turns '\\' into '/'; on Unix it is a no-op. Faithful to the platform build.
fn to_slash(path: &str) -> String {
    if MAIN_SEPARATOR == '/' {
        path.to_string()
    } else {
        path.replace(MAIN_SEPARATOR, "/")
    }
}

/// SourceDisc derives a short, deterministic, filesystem-safe token: the first
/// 6 hex chars of sha256(filepath.ToSlash(filepath.Dir(path))).
///
/// Dir-extraction decision: Go uses `filepath.ToSlash(filepath.Dir(path))`.
/// `filepath.Dir` treats both '/' and '\\' as separators on Windows. To be
/// faithful AND deterministic across platforms we normalize to forward slashes
/// first, strip a single trailing '/', then cut at the last '/'. This yields the
/// parent directory for the forward-slash test inputs and differs by parent dir,
/// satisfying the determinism + differs-by-path contracts.
pub fn source_disc(path: &str) -> String {
    let dir = dir_of(path);
    let mut hasher = Sha256::new();
    hasher.update(dir.as_bytes());
    let sum = hasher.finalize();
    let encoded = hex::encode(sum);
    encoded[..6].to_string()
}

/// Parent-directory extraction (see `source_disc` doc). Normalize to '/',
/// strip one trailing '/', cut at the last '/'.
fn dir_of(path: &str) -> String {
    let mut s = to_slash(path);
    if s.len() > 1 {
        s = s.trim_end_matches('/').to_string();
    }
    match s.rfind('/') {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

/// Base name of a path (Go filepath.Base semantics, sufficient for repo dirs).
fn basename_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Discover walks roots, finds repos, and classifies them. Read-only.
///
/// `include_nested == false` (default): descent stops at the first repo found, so
/// only top-level repos are returned. `true`: a found repo is recorded but the
/// walk continues into its working tree so nested repos are discovered too. A
/// directory literally named ".git" is always skipped in both modes.
pub fn discover(
    roots: &[String],
    scope: &Scope,
    exclude_dirs: &[String],
    include_nested: bool,
) -> Result<(Vec<Copy>, Vec<Copy>), GitError> {
    let skip: std::collections::HashSet<&str> =
        exclude_dirs.iter().map(|s| s.as_str()).collect();
    let run = gitx::new_runner();

    let mut in_scope: Vec<Copy> = Vec::new();
    let mut third_party: Vec<Copy> = Vec::new();

    for root in roots {
        let mut it = WalkDir::new(root).into_iter();
        loop {
            let entry = match it.next() {
                None => break,
                // Go tolerates unreadable dirs (returns nil on error): skip the
                // errored entry and keep walking.
                Some(Err(_)) => continue,
                Some(Ok(e)) => e,
            };
            // Only consider directories (Go: if !d.IsDir() { return nil }).
            if !entry.file_type().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            // Skip excluded dir subtrees (Go matches on d.Name()).
            if skip.contains(name.as_str()) {
                it.skip_current_dir();
                continue;
            }
            // A ".git" directory is never a repo and its subtree is skipped.
            if name == ".git" {
                it.skip_current_dir();
                continue;
            }
            let path = entry.path();
            if !gitx::is_repo(path) {
                continue;
            }
            // Found a repo.
            let path_str = path.to_string_lossy().to_string();
            let url = run
                .run(&path_str, &["config", "--get", "remote.origin.url"])
                .unwrap_or_default();
            let norm = normalize_url(&url);
            let (owner, repo) = parse_owner_repo(&norm, &basename_of(&path_str));
            let c = Copy {
                path: path_str.clone(),
                root: root.clone(),
                machine: infer_machine(&path_str),
                owner: owner.clone(),
                repo_name: repo,
                remote_url: norm,
                ..Default::default()
            };
            if is_third_party(&path_str, &owner, scope) {
                third_party.push(c);
            } else {
                in_scope.push(c);
            }
            if !include_nested {
                // Go: return filepath.SkipDir — stop descending into this repo.
                it.skip_current_dir();
            }
            // include_nested: keep descending (Go returns nil).
        }
    }
    Ok((in_scope, third_party))
}

/// isThirdParty: path under a third-party dir, or an out-of-scope owner.
pub(crate) fn is_third_party(path: &str, owner: &str, scope: &Scope) -> bool {
    for sub in &scope.third_party_dirs {
        if path.contains(sub) {
            return true;
        }
    }
    if owner.is_empty() {
        return false; // local-only repos are in scope
    }
    for o in &scope.in_scope_owners {
        // Go strings.EqualFold — ASCII case-insensitive suffices here.
        if o.eq_ignore_ascii_case(owner) {
            return false;
        }
    }
    true // has an owner not in our list -> third-party
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::MAIN_SEPARATOR;

    // Faithful port of Go TestNormalizeURL.
    #[test]
    fn test_normalize_url() {
        let cases = [
            ("git@github.com:inovacc/omni.git", "github.com/inovacc/omni"),
            ("https://github.com/inovacc/omni.git", "github.com/inovacc/omni"),
            ("https://github.com/inovacc/omni", "github.com/inovacc/omni"),
            (
                "ssh://git@github.com/lb-conn/treasury",
                "github.com/lb-conn/treasury",
            ),
        ];
        for (input, want) in cases {
            assert_eq!(normalize_url(input), want, "normalize_url({input:?})");
        }
    }

    // Faithful port of Go TestParseOwnerRepo.
    #[test]
    fn test_parse_owner_repo() {
        let (o, r) = parse_owner_repo("github.com/inovacc/omni", "omni");
        assert_eq!((o.as_str(), r.as_str()), ("inovacc", "omni"));
        let (o, r) = parse_owner_repo("", "auditor");
        assert_eq!((o.as_str(), r.as_str()), ("", "auditor"));
    }

    // Faithful port of Go TestInferMachine.
    #[test]
    fn test_infer_machine() {
        let cases = [
            ("D:/weaver-sync/development/personal/projects/x", "live"),
            ("D:/weaver-sync/New folder/acer/projects/x", "acer"),
            ("D:/weaver-sync/others/My Drive_2/dell/x", "dell"),
            ("D:/weaver-sync/others/My Drive_2/acer/x", "acer"),
        ];
        for (p, want) in cases {
            assert_eq!(infer_machine(p), want, "infer_machine({p:?})");
        }
    }

    // Faithful port of Go TestSourceDiscDeterministic.
    #[test]
    fn test_source_disc_deterministic() {
        let p = "D:/weaver-sync/New folder/acer/projects/x";
        let a = source_disc(p);
        let b = source_disc(p);
        assert_eq!(a, b, "source_disc not deterministic");
        assert!(!a.is_empty(), "source_disc returned empty token");
    }

    // Faithful port of Go TestSourceDiscDiffersByPath.
    #[test]
    fn test_source_disc_differs_by_path() {
        let a = source_disc("D:/weaver-sync/New folder/acer/projects/x");
        let b = source_disc("D:/weaver-sync/others/My Drive_2/acer/projects/x");
        assert_ne!(a, b, "source_disc collided for distinct source paths");
    }

    // Faithful port of Go TestIsThirdParty.
    #[test]
    fn test_is_third_party() {
        let sep = MAIN_SEPARATOR;
        let scope = Scope {
            in_scope_owners: vec!["inovacc".to_string(), "lb-conn".to_string()],
            third_party_dirs: vec![format!("{sep}public_repos{sep}")],
        };
        let third_party_path = format!("D:{sep}public_repos{sep}someupstream");
        let join = |parts: &[&str]| parts.join(&sep.to_string());
        let cases: [(&str, String, &str, bool); 5] = [
            ("path under third-party dir", third_party_path, "inovacc", true),
            (
                "local-only repo (no owner) is in scope",
                join(&["D:", "projects", "x"]),
                "",
                false,
            ),
            (
                "owner in scope list",
                join(&["D:", "projects", "omni"]),
                "inovacc",
                false,
            ),
            (
                "owner in scope list case-insensitive",
                join(&["D:", "projects", "omni"]),
                "INOVACC",
                false,
            ),
            (
                "owner not in scope list",
                join(&["D:", "projects", "other"]),
                "some-random-org",
                true,
            ),
        ];
        for (name, path, owner, want) in cases {
            assert_eq!(
                is_third_party(&path, owner, &scope),
                want,
                "case: {name}"
            );
        }
    }

    fn contains_path(copies: &[Copy], path: &str) -> bool {
        copies.iter().any(|c| c.path == path)
    }

    // Faithful port of Go TestDiscoverNestedRepos.
    // NOTE: these are bare .git DIRS, not real repos, so `git config` returns an
    // error -> url "". This test EXECs real git and touches the FS.
    #[test]
    fn test_discover_nested_repos() {
        use std::fs;
        // Unique temp dir under the system temp dir (Go t.TempDir()).
        let base = std::env::temp_dir().join(format!(
            "reposmerge_discover_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let outer = base.join("outer");
        let inner = outer.join("sub").join("inner");
        let plain = outer.join("sub").join("plain");

        fs::create_dir_all(outer.join(".git")).unwrap();
        fs::create_dir_all(inner.join(".git")).unwrap();
        fs::create_dir_all(&plain).unwrap();

        let base_str = base.to_string_lossy().to_string();
        let outer_str = outer.to_string_lossy().to_string();
        let inner_str = inner.to_string_lossy().to_string();
        let dotgit_str = outer.join(".git").to_string_lossy().to_string();

        // default: excludes nested repos.
        let (in_scope, _tp) =
            discover(&[base_str.clone()], &default_scope(), &[], false).unwrap();
        assert!(
            contains_path(&in_scope, &outer_str),
            "expected outer repo in results"
        );
        assert!(
            !contains_path(&in_scope, &inner_str),
            "did not expect inner repo when include_nested=false"
        );
        assert!(
            !contains_path(&in_scope, &dotgit_str),
            ".git dir itself must never be returned as a repo"
        );

        // include_nested: finds both outer and inner.
        let (in_scope, _tp) =
            discover(&[base_str.clone()], &default_scope(), &[], true).unwrap();
        assert!(
            contains_path(&in_scope, &outer_str),
            "expected outer repo in results"
        );
        assert!(
            contains_path(&in_scope, &inner_str),
            "expected inner repo when include_nested=true"
        );
        assert!(
            !contains_path(&in_scope, &dotgit_str),
            ".git dir itself must never be returned as a repo"
        );

        // Best-effort cleanup.
        let _ = fs::remove_dir_all(&base);
    }
}
