//! Faithful 1:1 port of Go `internal/model/model.go`.
//!
//! These types are serialized later by the `report` module via Go
//! `json.MarshalIndent` with NO json tags, so JSON keys are exact PascalCase.
//! Each field carries `#[serde(rename = "...")]` to reproduce those keys.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Zero `time.Time` equivalent: Go's zero value marshals as
/// `"0001-01-01T00:00:00Z"`. chrono can represent year 1, so this reproduces
/// Go's zero-value semantics. PARITY-VERIFY: confirm RFC3339 formatting matches
/// once the `report` module lands (see PORT-TRACK deviations).
fn zero_time() -> DateTime<Utc> {
    DateTime::<Utc>::from_naive_utc_and_offset(
        chrono::NaiveDate::from_ymd_opt(1, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap(),
        Utc,
    )
}

/// Copy is one on-disk working-tree of a repo found during discovery.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Copy {
    /// absolute path to the repo dir (parent of .git)
    #[serde(rename = "Path")]
    pub path: String,
    /// scan root this copy was found under
    #[serde(rename = "Root")]
    pub root: String,
    /// inferred source label: live, acer, dell, drive, unknown
    #[serde(rename = "Machine")]
    pub machine: String,
    /// org parsed from RemoteURL, or "" for local-only
    #[serde(rename = "Owner")]
    pub owner: String,
    /// repo name from RemoteURL, else basename
    #[serde(rename = "RepoName")]
    pub repo_name: String,
    /// normalized origin URL, "" if none
    #[serde(rename = "RemoteURL")]
    pub remote_url: String,
    /// filled by the fingerprint package
    #[serde(rename = "FP")]
    pub fp: Fingerprint,
}

/// Branch is a local branch and its tip SHA.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Tip")]
    pub tip: String,
}

/// Fingerprint captures the git state of one Copy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fingerprint {
    /// HEAD sha; "" for an empty repo
    #[serde(rename = "Head")]
    pub head: String,
    /// root-commit shas, sorted (lineage identity)
    #[serde(rename = "RootCommits")]
    pub root_commits: Vec<String>,
    /// all reachable commit shas across all refs, sorted
    #[serde(rename = "AllCommits")]
    pub all_commits: Vec<String>,
    /// local branches
    #[serde(rename = "Branches")]
    pub branches: Vec<Branch>,
    /// commits ahead of origin's matching branch
    #[serde(rename = "Ahead")]
    pub ahead: i64,
    /// commits behind
    #[serde(rename = "Behind")]
    pub behind: i64,
    /// modified + staged files
    #[serde(rename = "DirtyCount")]
    pub dirty_count: i64,
    /// untracked, non-ignored files
    #[serde(rename = "UntrackedCount")]
    pub untracked_count: i64,
    /// entries in the stash
    #[serde(rename = "StashCount")]
    pub stash_count: i64,
    /// len(AllCommits)
    #[serde(rename = "CommitCount")]
    pub commit_count: i64,
    /// author date of HEAD
    #[serde(rename = "LastCommit")]
    pub last_commit: DateTime<Utc>,
    /// bytes, generated dirs excluded
    #[serde(rename = "WorktreeSize")]
    pub worktree_size: i64,
    /// mtime of the repo dir
    #[serde(rename = "DirMtime")]
    pub dir_mtime: DateTime<Utc>,
}

impl Default for Fingerprint {
    fn default() -> Self {
        Fingerprint {
            head: String::new(),
            root_commits: Vec::new(),
            all_commits: Vec::new(),
            branches: Vec::new(),
            ahead: 0,
            behind: 0,
            dirty_count: 0,
            untracked_count: 0,
            stash_count: 0,
            commit_count: 0,
            last_commit: zero_time(),
            worktree_size: 0,
            dir_mtime: zero_time(),
        }
    }
}

impl Fingerprint {
    /// Score ranks copies for canonical selection. Higher wins.
    /// Priority: unpushed commits >> dirty >> untracked >> stashes >> recency.
    ///
    /// Go uses platform `int` (64-bit here); ported as i64 to match exactly.
    pub fn score(&self) -> i64 {
        let mut s = self.ahead * 1_000_000;
        if self.dirty_count > 0 {
            s += 100_000 + self.dirty_count;
        }
        if self.untracked_count > 0 {
            s += 10_000 + self.untracked_count;
        }
        s += self.stash_count * 50_000;
        // day granularity tie-break: Go int(LastCommit.Unix() / 86_400)
        s += self.last_commit.timestamp() / 86_400;
        s
    }
}

/// StrategyKind enumerates reconciliation strategies.
/// Serializes to the exact Go string values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyKind {
    /// remote-backed
    #[serde(rename = "A-richest-quarantine")]
    A,
    /// local-only, shared lineage
    #[serde(rename = "B-union-branches")]
    B,
    /// collision / unclassified
    #[serde(rename = "C-snapshot")]
    C,
}

impl Default for StrategyKind {
    fn default() -> Self {
        StrategyKind::A
    }
}

/// Group is a set of Copies that are the same logical repo.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Group {
    /// remote URL, or "noremote:<rootsha>:<name>"
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Owner")]
    pub owner: String,
    #[serde(rename = "RepoName")]
    pub repo_name: String,
    #[serde(rename = "HasRemote")]
    pub has_remote: bool,
    #[serde(rename = "RemoteURL")]
    pub remote_url: String,
    #[serde(rename = "Copies")]
    pub copies: Vec<Copy>,
}

/// QuarantineItem is a divergent copy preserved side-by-side.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QuarantineItem {
    #[serde(rename = "Copy")]
    pub copy: Copy,
    /// _quarantine/<repo>/<machine>
    #[serde(rename = "DestPath")]
    pub dest_path: String,
    /// unreachable-commits | dirty | different-lineage
    #[serde(rename = "Reason")]
    pub reason: String,
    /// SHAs present here but not in canonical
    #[serde(rename = "UnreachableCommits")]
    pub unreachable_commits: Vec<String>,
}

/// UnionRemote is a non-canonical copy folded into canonical as branches (Strategy B).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UnionRemote {
    /// consolidate-<machine>
    #[serde(rename = "Name")]
    pub name: String,
    /// source copy path
    #[serde(rename = "Path")]
    pub path: String,
    /// branches to preserve as consolidate/<machine>/<branch>
    #[serde(rename = "Branches")]
    pub branches: Vec<String>,
}

/// Decision is the planned action for one Group.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    #[serde(rename = "Group")]
    pub group: Group,
    #[serde(rename = "Strategy")]
    pub strategy: StrategyKind,
    #[serde(rename = "Canonical")]
    pub canonical: Copy,
    #[serde(rename = "CanonicalReason")]
    pub canonical_reason: String,
    /// canonical/<owner>/<repo>
    #[serde(rename = "DestPath")]
    pub dest_path: String,
    #[serde(rename = "Quarantine")]
    pub quarantine: Vec<QuarantineItem>,
    /// strict-subset copy paths (safe to delete; NOT deleted)
    #[serde(rename = "Redundant")]
    pub redundant: Vec<String>,
    #[serde(rename = "UnionRemotes")]
    pub union_remotes: Vec<UnionRemote>,
}

/// ApplyResult summarizes an apply run (defined here to avoid import cycles).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ApplyResult {
    #[serde(rename = "Copied")]
    pub copied: i64,
    #[serde(rename = "Quarantined")]
    pub quarantined: i64,
    #[serde(rename = "Unioned")]
    pub unioned: i64,
    #[serde(rename = "Skipped")]
    pub skipped: i64,
    #[serde(rename = "SkippedFiles")]
    pub skipped_files: Vec<String>,
    #[serde(rename = "Actions")]
    pub actions: Vec<String>,
}

/// Plan is the full set of decisions for a run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    #[serde(rename = "Roots")]
    pub roots: Vec<String>,
    #[serde(rename = "Dest")]
    pub dest: String,
    /// RFC3339; injected by the command layer
    #[serde(rename = "GeneratedAt")]
    pub generated_at: String,
    #[serde(rename = "Decisions")]
    pub decisions: Vec<Decision>,
    /// inventory only
    #[serde(rename = "ThirdParty")]
    pub third_party: Vec<Copy>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Faithful port of Go TestScoreOrdersByAhead.
    #[test]
    fn test_score_orders_by_ahead() {
        let a = Fingerprint {
            ahead: 5,
            dirty_count: 0,
            ..Default::default()
        };
        let b = Fingerprint {
            ahead: 1,
            dirty_count: 9,
            ..Default::default()
        };
        assert!(
            a.score() > b.score(),
            "ahead=5 ({}) should outrank ahead=1 dirty=9 ({})",
            a.score(),
            b.score()
        );
    }
}
