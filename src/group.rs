//! Faithful 1:1 port of Go `internal/group/group.go`.
//!
//! Groups discovered `Copy`s into logical repos. Pure module, std-only.

use std::collections::HashMap;

use crate::model::{Copy, Group};

/// Build groups copies into logical repos.
///
/// Faithful to Go `Build`: group keys are emitted in FIRST-SEEN insertion
/// order. Go tracks an `order []string` alongside a `map[string]*Group`; we
/// reproduce that with an `order: Vec<String>` + `HashMap<String, Group>` so
/// output ordering is deterministic (HashMap iteration order is NOT used).
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
        g.copies.push(c);
    }

    let mut out: Vec<Group> = Vec::with_capacity(order.len());
    for k in &order {
        out.push(by_key[k].clone());
    }
    out
}

/// groupKey is the remote URL when present, else name+lineage fingerprint.
fn group_key(c: &Copy) -> String {
    if !c.remote_url.is_empty() {
        format!("remote:{}", c.remote_url)
    } else {
        format!("noremote:{}:{}", c.repo_name, lineage(c))
    }
}

/// lineage is a stable signature of the root-commit set (project identity).
fn lineage(c: &Copy) -> String {
    let mut rc = c.fp.root_commits.clone();
    rc.sort(); // byte-lexicographic == Go sort.Strings
    if rc.is_empty() {
        "EMPTY".to_string() // empty/uninit repos keyed only by name
    } else {
        rc.join(",")
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
}
