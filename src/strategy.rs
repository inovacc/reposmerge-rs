//! Faithful 1:1 port of Go `internal/strategy` (github.com/inovacc/reposmerge).
//!
//! Produces the planned action (`Decision`) for a `Group`. Std-only; consumes
//! `crate::model` types and `crate::discover::source_disc`.

use std::collections::{HashMap, HashSet};

use crate::discover;
use crate::model::{Copy, Decision, Group, QuarantineItem, StrategyKind, UnionRemote};

/// Faithful port of Go `path.Join`: join the non-empty elements with '/', then
/// run `path.Clean`. Forward-slash only, even on Windows. This matters at runtime:
/// the CLI's default `dest_root` is `"./canonical"`, and Go's `path.Clean` strips
/// the leading `./` (so `DestPath` is `canonical/...`, not `./canonical/...`).
fn path_join(parts: &[&str]) -> String {
    let joined = parts
        .iter()
        .filter(|e| !e.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("/");
    path_clean(&joined)
}

/// Faithful port of Go `path.Clean` (lexical, forward-slash). Returns the
/// shortest equivalent path: collapses multiple slashes, drops `.` elements,
/// resolves `..` against the accumulated output, and removes a trailing slash.
/// An empty path cleans to `.`.
fn path_clean(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let b = path.as_bytes();
    let n = b.len();
    let rooted = b[0] == b'/';
    let mut out: Vec<u8> = Vec::with_capacity(n);
    // `dotdot` marks the point in `out` before which `..` cannot backtrack.
    let mut r = 0usize;
    let mut dotdot = 0usize;
    if rooted {
        out.push(b'/');
        r = 1;
        dotdot = 1;
    }
    while r < n {
        if b[r] == b'/' {
            // empty path element
            r += 1;
        } else if b[r] == b'.' && (r + 1 == n || b[r + 1] == b'/') {
            // `.` element
            r += 1;
        } else if b[r] == b'.' && b[r + 1] == b'.' && (r + 2 == n || b[r + 2] == b'/') {
            // `..` element: remove to last '/'
            r += 2;
            if out.len() > dotdot {
                let mut w = out.len() - 1;
                while w > dotdot && out[w] != b'/' {
                    w -= 1;
                }
                out.truncate(w);
            } else if !rooted {
                if !out.is_empty() {
                    out.push(b'/');
                }
                out.push(b'.');
                out.push(b'.');
                dotdot = out.len();
            }
        } else {
            // real path element: add slash if needed, then copy it
            if (rooted && out.len() != 1) || (!rooted && !out.is_empty()) {
                out.push(b'/');
            }
            while r < n && b[r] != b'/' {
                out.push(b[r]);
                r += 1;
            }
        }
    }
    if out.is_empty() {
        return ".".to_string();
    }
    String::from_utf8(out).unwrap()
}

/// Decide produces the planned action for a group.
pub fn decide(g: &Group, dest_root: &str) -> Decision {
    let (canonical, reason) = choose_canonical(&g.copies);
    let owner = if g.owner.is_empty() {
        "personal"
    } else {
        &g.owner
    };

    let mut d = Decision {
        group: g.clone(),
        canonical: canonical.clone(),
        canonical_reason: reason,
        dest_path: path_join(&[dest_root, owner, &g.repo_name]),
        ..Default::default()
    };

    let mut brand_count: HashMap<String, i64> = HashMap::with_capacity(g.copies.len());
    for c in &g.copies {
        *brand_count.entry(c.machine.clone()).or_insert(0) += 1;
    }

    if g.has_remote {
        d.strategy = StrategyKind::A;
        for c in &g.copies {
            if c.path == canonical.path {
                continue;
            }
            let missing = unreachable(&canonical, c);
            if !missing.is_empty()
                || c.fp.dirty_count > 0
                || c.fp.untracked_count > 0
                || c.fp.stash_count > 0
            {
                let collides = brand_count.get(&c.machine).copied().unwrap_or(0) > 1;
                d.quarantine.push(QuarantineItem {
                    copy: c.clone(),
                    dest_path: path_join(&["_quarantine", &g.repo_name, &label(c, collides)]),
                    reason: reason_for(&missing, c),
                    unreachable_commits: missing,
                });
            } else {
                d.redundant.push(c.path.clone());
            }
        }
    } else {
        d.strategy = StrategyKind::B;
        for c in &g.copies {
            if c.path == canonical.path {
                continue;
            }
            let branches: Vec<String> = c.fp.branches.iter().map(|b| b.name.clone()).collect();
            let collides = brand_count.get(&c.machine).copied().unwrap_or(0) > 1;
            d.union_remotes.push(UnionRemote {
                name: format!("consolidate-{}", label(c, collides)),
                path: c.path.clone(),
                branches,
            });
        }
    }
    d
}

/// label returns the copy's machine brand, disambiguated with a path-derived
/// source discriminator when that brand collides with another copy in the same
/// group. Non-colliding brands stay bare for backwards compatibility.
fn label(c: &Copy, collides: bool) -> String {
    if !collides {
        c.machine.clone()
    } else {
        format!("{}-{}", c.machine, discover::source_disc(&c.path))
    }
}

/// choose_canonical picks the richest copy; ties broken by machine priority
/// live>acer>dell>drive. Assumes `copies` is non-empty (Go `copies[0]`).
pub fn choose_canonical(copies: &[Copy]) -> (Copy, String) {
    let mut best = &copies[0];
    for c in &copies[1..] {
        if c.fp.score() > best.fp.score()
            || (c.fp.score() == best.fp.score() && machine_rank(&c.machine) < machine_rank(&best.machine))
        {
            best = c;
        }
    }
    (
        best.clone(),
        format!("highest richness score; machine={}", best.machine),
    )
}

/// unreachable returns commits present in `other` but not in `canonical`,
/// preserving `other`'s order.
pub fn unreachable(canonical: &Copy, other: &Copy) -> Vec<String> {
    let have: HashSet<&String> = canonical.fp.all_commits.iter().collect();
    other
        .fp
        .all_commits
        .iter()
        .filter(|s| !have.contains(s))
        .cloned()
        .collect()
}

fn reason_for(missing: &[String], c: &Copy) -> String {
    if !missing.is_empty() {
        "unreachable-commits".to_string()
    } else if c.fp.dirty_count > 0 {
        "uncommitted-changes".to_string()
    } else if c.fp.untracked_count > 0 {
        "untracked-files".to_string()
    } else {
        "stashed-changes".to_string()
    }
}

fn machine_rank(m: &str) -> i32 {
    match m {
        "live" => 0,
        "acer" => 1,
        "dell" => 2,
        "drive" => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Fingerprint;

    fn mk(machine: &str, ahead: i64, commits: &[&str]) -> Copy {
        Copy {
            machine: machine.to_string(),
            repo_name: "x".to_string(),
            path: format!("/path/{machine}"),
            fp: Fingerprint {
                ahead,
                all_commits: commits.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn mk_at(machine: &str, p: &str, ahead: i64, commits: &[&str]) -> Copy {
        Copy {
            machine: machine.to_string(),
            repo_name: "x".to_string(),
            path: p.to_string(),
            fp: Fingerprint {
                ahead,
                all_commits: commits.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_remote_group_uses_strategy_a() {
        let g = Group {
            has_remote: true,
            repo_name: "omni".to_string(),
            owner: "inovacc".to_string(),
            remote_url: "github.com/inovacc/omni".to_string(),
            copies: vec![
                mk("live", 3, &["a", "b", "c"]),
                mk("acer", 0, &["a", "b"]),      // strict subset -> redundant
                mk("dell", 0, &["a", "b", "z"]), // has unique 'z' -> quarantine
            ],
            ..Default::default()
        };
        let d = decide(&g, "canonical");
        assert_eq!(d.strategy, StrategyKind::A);
        assert_eq!(d.canonical.machine, "live");
        assert_eq!(d.quarantine.len(), 1);
        assert_eq!(d.quarantine[0].copy.machine, "dell");
        assert_eq!(d.redundant.len(), 1);
        assert_eq!(d.dest_path, "canonical/inovacc/omni");
    }

    #[test]
    fn test_stash_only_copy_is_quarantined_not_redundant() {
        // live has 1 unpushed commit (Ahead=1) so it wins canonical.
        // acer has same commits but a stash — must be quarantined, not redundant.
        let canon = Copy {
            path: "/live".to_string(),
            machine: "live".to_string(),
            repo_name: "x".to_string(),
            fp: Fingerprint {
                ahead: 1,
                all_commits: vec!["a".to_string(), "b".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let stashed = Copy {
            path: "/acer".to_string(),
            machine: "acer".to_string(),
            repo_name: "x".to_string(),
            fp: Fingerprint {
                all_commits: vec!["a".to_string(), "b".to_string()],
                stash_count: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        let g = Group {
            has_remote: true,
            repo_name: "x".to_string(),
            owner: "o".to_string(),
            remote_url: "github.com/o/x".to_string(),
            copies: vec![canon, stashed],
            ..Default::default()
        };
        let d = decide(&g, "canonical");
        assert_eq!(d.quarantine.len(), 1);
        assert_eq!(d.quarantine[0].copy.path, "/acer");
        assert_eq!(d.redundant.len(), 0);
    }

    #[test]
    fn test_local_only_group_uses_strategy_b() {
        let g = Group {
            has_remote: false,
            repo_name: "auditor".to_string(),
            copies: vec![mk("live", 0, &["a", "b"]), mk("acer", 0, &["a", "c"])],
            ..Default::default()
        };
        let d = decide(&g, "canonical");
        assert_eq!(d.strategy, StrategyKind::B);
        assert_eq!(d.union_remotes.len(), 1);
        assert_eq!(d.dest_path, "canonical/personal/auditor");
    }

    #[test]
    fn test_remote_group_same_brand_collision_gets_distinct_quarantine_labels() {
        let canon = mk_at("live", "/live/omni", 3, &["a", "b", "c"]);
        let acer1 = mk_at("acer", "/New folder/acer/projects/x", 0, &["a", "b", "z1"]);
        let acer2 = mk_at("acer", "/others/My Drive_2/acer/x", 0, &["a", "b", "z2"]);
        let g = Group {
            has_remote: true,
            repo_name: "omni".to_string(),
            owner: "inovacc".to_string(),
            remote_url: "github.com/inovacc/omni".to_string(),
            copies: vec![canon, acer1, acer2],
            ..Default::default()
        };
        let d = decide(&g, "canonical");
        assert_eq!(d.quarantine.len(), 2);
        let dest1 = &d.quarantine[0].dest_path;
        let dest2 = &d.quarantine[1].dest_path;
        assert_ne!(dest1, dest2, "colliding acer quarantine DestPaths must be distinct");
        const PREFIX: &str = "_quarantine/omni/acer-";
        for dest in [dest1, dest2] {
            assert!(
                dest.len() > PREFIX.len() && dest.starts_with(PREFIX),
                "expected quarantine DestPath to start with {PREFIX:?}, got {dest:?}"
            );
        }
    }

    #[test]
    fn test_local_only_group_same_brand_collision_gets_distinct_union_labels() {
        let canon = mk_at("live", "/live/x", 0, &["a", "b"]);
        let acer1 = mk_at("acer", "/New folder/acer/x", 0, &["a", "b"]);
        let acer2 = mk_at("acer", "/others/My Drive_2/acer/x", 0, &["a", "b"]);
        let g = Group {
            has_remote: false,
            repo_name: "auditor".to_string(),
            copies: vec![canon, acer1, acer2],
            ..Default::default()
        };
        let d = decide(&g, "canonical");
        assert_eq!(d.union_remotes.len(), 2);
        let name1 = &d.union_remotes[0].name;
        let name2 = &d.union_remotes[1].name;
        assert_ne!(name1, name2, "colliding acer union remote names must be distinct");
        const PREFIX: &str = "consolidate-acer-";
        for name in [name1, name2] {
            assert!(
                name.len() > PREFIX.len() && name.starts_with(PREFIX),
                "expected union remote name to start with {PREFIX:?}, got {name:?}"
            );
        }
    }

    #[test]
    fn test_no_collision_labels_remain_bare() {
        let canon = mk_at("live", "/live/x", 3, &["a", "b", "c"]);
        let dell = mk_at("dell", "/dell/x", 0, &["a", "b", "z"]);
        let g = Group {
            has_remote: true,
            repo_name: "omni".to_string(),
            owner: "inovacc".to_string(),
            remote_url: "github.com/inovacc/omni".to_string(),
            copies: vec![canon, dell],
            ..Default::default()
        };
        let d = decide(&g, "canonical");
        assert_eq!(d.quarantine.len(), 1);
        assert_eq!(d.quarantine[0].dest_path, "_quarantine/omni/dell");

        let g_b = Group {
            has_remote: false,
            repo_name: "auditor".to_string(),
            copies: vec![
                mk_at("live", "/live/x", 0, &["a", "b"]),
                mk_at("dell", "/dell/x", 0, &["a", "b"]),
            ],
            ..Default::default()
        };
        let d_b = decide(&g_b, "canonical");
        assert_eq!(d_b.union_remotes.len(), 1);
        assert_eq!(d_b.union_remotes[0].name, "consolidate-dell");
    }
}
