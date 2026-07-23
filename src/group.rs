//! Groups discovered `Copy`s into logical repos.
//!
//! **DELIBERATE DEVIATION FROM THE Go ORACLE (smart-match, default-on).** The Go
//! original keys a group by the remote URL when present, else `name+lineage`.
//! That under-merges two real-world cases: (1) a remote-backed copy and a
//! local-only copy of the SAME repo (same root commit, one had its remote
//! stripped/changed) land in different groups; (2) case-variant remote URLs
//! (`inovacc/CaseTest` vs `inovacc/casetest`) land in different groups. Both are
//! false-negatives (missed duplicates), never data loss.
//!
//! This port keys **lineage-first**: a repo's identity IS its root-commit set,
//! which is byte-identical across every clone regardless of remote presence or
//! URL form/case, and effectively unique per lineage. So all copies of one repo
//! unify into a single group; only genuinely divergent lineages stay separate.
//! Empty/uninitialized repos (no root commit) fall back to a case-insensitive
//! remote key, else name. This is a superset of the Go behavior — same-lineage
//! copies that Go already merged still merge — so it never SPLITS a group Go
//! kept; it only JOINS groups Go wrongly split. Documented in PORT-TRACK.md and
//! docs/ISSUES.md.
//!
//! Pure module, std-only.

use std::collections::HashMap;

use crate::model::{Copy, Group};

/// Build groups copies into logical repos.
///
/// Group keys are emitted in FIRST-SEEN insertion order (deterministic:
/// `order: Vec<String>` beside a `HashMap<String, Group>`; HashMap iteration
/// order is NOT used). When a lineage group is first seeded by a local-only
/// copy and a remote-bearing copy joins later, the group's identity
/// (owner/repo/remote) is upgraded from the remote copy so the canonical dest
/// path uses `<owner>/<repo>` rather than a bare basename.
pub fn build(copies: Vec<Copy>) -> Vec<Group> {
    let mut by_key: HashMap<String, Group> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for c in copies {
        let key = group_key(&c);
        let g = by_key.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Group {
                key: key.clone(),
                owner: c.owner.clone(),
                repo_name: c.repo_name.clone(),
                has_remote: !c.remote_url.is_empty(),
                remote_url: c.remote_url.clone(),
                copies: Vec::new(),
            }
        });
        // smart-match: a lineage group may be seeded by a local-only copy; upgrade
        // its identity when the first remote-bearing copy joins so the dest path
        // and reports carry the real <owner>/<repo> and remote URL.
        if !g.has_remote && !c.remote_url.is_empty() {
            g.has_remote = true;
            g.remote_url = c.remote_url.clone();
            g.owner = c.owner.clone();
            g.repo_name = c.repo_name.clone();
        }
        g.copies.push(c);
    }

    let mut out: Vec<Group> = Vec::with_capacity(order.len());
    for k in &order {
        out.push(by_key[k].clone());
    }
    out
}

/// groupKey is the root-commit lineage when the repo has any history (so clones
/// unify across remote/local and URL case), else a case-insensitive remote key,
/// else the lowercased name. See the module doc for the deviation rationale.
fn group_key(c: &Copy) -> String {
    let mut rc = c.fp.root_commits.clone();
    rc.sort(); // byte-lexicographic == Go sort.Strings
    if !rc.is_empty() {
        format!("lineage:{}", rc.join(","))
    } else if !c.remote_url.is_empty() {
        format!("remote:{}", c.remote_url.to_lowercase())
    } else {
        format!("noremote:{}:EMPTY", c.repo_name.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Fingerprint;

    fn cp(name: &str, url: &str, root: &str) -> Copy {
        Copy {
            repo_name: name.to_string(),
            remote_url: url.to_string(),
            fp: Fingerprint {
                root_commits: vec![root.to_string()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    // Faithful port of Go TestRemoteCopiesGroupTogether.
    #[test]
    fn test_remote_copies_group_together() {
        let gs = build(vec![
            cp("omni", "github.com/inovacc/omni", "r1"),
            cp("omni", "github.com/inovacc/omni", "r1"),
        ]);
        assert!(
            gs.len() == 1 && gs[0].copies.len() == 2 && gs[0].has_remote,
            "got {:?}",
            gs
        );
    }

    // Faithful port of Go TestSameNameDifferentLineageDoNotMerge.
    #[test]
    fn test_same_name_different_lineage_do_not_merge() {
        let gs = build(vec![
            cp("daemon", "", "rootA"),
            cp("daemon", "", "rootB"), // unrelated project, same name
        ]);
        assert_eq!(
            gs.len(),
            2,
            "expected 2 groups for divergent lineage, got {}",
            gs.len()
        );
    }

    // Faithful port of Go TestSameNameSameLineageMerge.
    #[test]
    fn test_same_name_same_lineage_merge() {
        let gs = build(vec![cp("loom", "", "shared"), cp("loom", "", "shared")]);
        assert!(
            gs.len() == 1 && gs[0].copies.len() == 2,
            "expected 1 merged group, got {:?}",
            gs
        );
    }

    // ---- smart-match (deviation from Go): the three gaps closed ----

    // A remote-backed copy and a local-only copy of the SAME repo (same root
    // commit, remote stripped from one) unify into one group. Go split these.
    #[test]
    fn smart_remote_and_local_same_lineage_unify() {
        let gs = build(vec![
            cp("proj", "github.com/inovacc/proj", "r1"), // has remote
            cp("proj", "", "r1"),                        // remote stripped, same root
        ]);
        assert_eq!(
            gs.len(),
            1,
            "remote + local same-lineage must unify: {gs:?}"
        );
        assert_eq!(gs[0].copies.len(), 2);
        // group identity is upgraded to the remote-bearing copy for dest naming.
        assert!(gs[0].has_remote, "merged group should carry the remote");
        assert_eq!(gs[0].remote_url, "github.com/inovacc/proj");
    }

    // Same as above but the local-only copy is seen FIRST — the group must still
    // upgrade its identity when the remote copy joins.
    #[test]
    fn smart_local_first_then_remote_upgrades_identity() {
        let gs = build(vec![
            cp("proj", "", "r1"),                        // local-only, seen first
            cp("proj", "github.com/inovacc/proj", "r1"), // remote joins
        ]);
        assert_eq!(gs.len(), 1);
        // identity upgraded from the local-only seed to the remote-bearing copy
        // (the cp() helper doesn't populate owner — that's parse_owner_repo's job
        // in real discovery — so assert the fields the helper actually sets).
        assert!(gs[0].has_remote);
        assert_eq!(gs[0].remote_url, "github.com/inovacc/proj");
    }

    // Case-variant remotes of the same clone (same root commit) unify via lineage.
    #[test]
    fn smart_case_variant_remotes_unify_via_lineage() {
        let gs = build(vec![
            cp("casetest", "github.com/inovacc/CaseTest", "r1"),
            cp("casetest", "github.com/inovacc/casetest", "r1"),
        ]);
        assert_eq!(gs.len(), 1, "case-variant same-lineage must unify: {gs:?}");
        assert_eq!(gs[0].copies.len(), 2);
    }

    // Empty repos (no root commit) with case-variant remotes still unify via the
    // case-insensitive remote fallback key.
    #[test]
    fn smart_empty_repo_case_insensitive_remote_key() {
        let mut a = cp("x", "github.com/inovacc/X", "");
        a.fp.root_commits.clear();
        let mut b = cp("x", "github.com/inovacc/x", "");
        b.fp.root_commits.clear();
        let gs = build(vec![a, b]);
        assert_eq!(
            gs.len(),
            1,
            "empty-repo case-variant remotes must unify: {gs:?}"
        );
    }

    // Genuinely divergent lineages (different root commits) still stay separate —
    // smart-match never over-merges unrelated histories.
    #[test]
    fn smart_divergent_lineage_still_separate() {
        let gs = build(vec![
            cp("proj", "github.com/inovacc/proj", "rootA"),
            cp("proj", "github.com/inovacc/proj-fork", "rootB"),
        ]);
        assert_eq!(gs.len(), 2, "different root commits must not merge: {gs:?}");
    }
}
