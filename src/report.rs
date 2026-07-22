//! Faithful 1:1 port of Go `internal/report/report.go`.
//!
//! Emits the byte-exact report artifacts: inventory CSVs, `plan.json` (+
//! `divergence.md`), `checksums.sha256`, and `MANIFEST.md`. This is the
//! highest parity-risk module: `plan.json`, `divergence.md`, and `MANIFEST.md`
//! are compared byte-for-byte against checked-in goldens.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::model;

/// `reportsDir`: join `dir/reports`, `MkdirAll`, return it.
fn reports_dir(dir: &Path) -> io::Result<PathBuf> {
    let d = dir.join("reports");
    fs::create_dir_all(&d)?;
    Ok(d)
}

/// Go `fmt %v` on a `[]string`: `[a b c]` (space-separated, square brackets);
/// empty slice -> `[]`.
fn go_slice_v(items: &[String]) -> String {
    format!("[{}]", items.join(" "))
}

/// Render a `StrategyKind` as its Go `String()` value (matches the serde
/// rename used for JSON), reproducing Go's `%s` on the strategy.
fn strategy_str(s: model::StrategyKind) -> &'static str {
    match s {
        model::StrategyKind::A => "A-richest-quarantine",
        model::StrategyKind::B => "B-union-branches",
        model::StrategyKind::C => "C-snapshot",
    }
}

/// WriteInventory emits inventory.csv (in-scope) and third-party.csv.
pub fn write_inventory(
    dir: &Path,
    in_scope: &[model::Copy],
    third_party: &[model::Copy],
) -> io::Result<()> {
    let d = reports_dir(dir)?;
    write_csv(&d.join("inventory.csv"), in_scope)?;
    write_csv(&d.join("third-party.csv"), third_party)
}

fn write_csv(path: &Path, copies: &[model::Copy]) -> io::Result<()> {
    // Go `encoding/csv` default terminator is `\n` (UseCRLF=false); the `csv`
    // crate defaults to CRLF, so force LF for byte parity with the source.
    let mut w = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_path(path)
        .map_err(csv_io)?;
    w.write_record([
        "owner",
        "repo",
        "machine",
        "remote",
        "head",
        "commits",
        "ahead",
        "dirty",
        "untracked",
        "path",
    ])
    .map_err(csv_io)?;
    for c in copies {
        w.write_record([
            c.owner.clone(),
            c.repo_name.clone(),
            c.machine.clone(),
            c.remote_url.clone(),
            c.fp.head.clone(),
            c.fp.commit_count.to_string(),
            c.fp.ahead.to_string(),
            c.fp.dirty_count.to_string(),
            c.fp.untracked_count.to_string(),
            c.path.clone(),
        ])
        .map_err(csv_io)?;
    }
    w.flush()?;
    Ok(())
}

fn csv_io(e: csv::Error) -> io::Error {
    io::Error::other(e)
}

/// WritePlan emits plan.json and a human divergence.md.
pub fn write_plan(dir: &Path, p: &model::Plan) -> io::Result<()> {
    let d = reports_dir(dir)?;
    // Go json.MarshalIndent(p, "", "  ") -> 2-space indent, struct-declaration
    // key order, NO trailing newline. serde_json::to_string_pretty matches the
    // indentation and (with declaration-ordered fields) the key order.
    let json = serde_json::to_string_pretty(p)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(d.join("plan.json"), json.as_bytes())?;

    let mut sb = String::new();
    sb.push_str("# Divergence report\n\n");
    for dec in &p.decisions {
        if dec.quarantine.is_empty() && dec.union_remotes.is_empty() {
            continue;
        }
        sb.push_str(&format!(
            "## {}/{} — strategy {}\n",
            dec.group.owner,
            dec.group.repo_name,
            strategy_str(dec.strategy)
        ));
        sb.push_str(&format!(
            "- canonical: {} ({})\n",
            dec.canonical.machine, dec.canonical_reason
        ));
        for q in &dec.quarantine {
            sb.push_str(&format!(
                "- quarantine {}: {} ({} unreachable commits)\n",
                q.copy.machine,
                q.reason,
                q.unreachable_commits.len()
            ));
        }
        for u in &dec.union_remotes {
            sb.push_str(&format!(
                "- union {}: branches {}\n",
                u.name,
                go_slice_v(&u.branches)
            ));
        }
        sb.push('\n');
    }
    fs::write(d.join("divergence.md"), sb.as_bytes())
}

/// LoadPlan reads a plan.json.
pub fn load_plan(path: &Path) -> io::Result<model::Plan> {
    let b = fs::read(path)?;
    serde_json::from_slice(&b).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// WriteChecksums walks every file under `dest` (including `.git`) and writes
/// `dir/reports/checksums.sha256` in `sha256sum` format: `<hex>  <relpath>\n`,
/// relpaths forward-slashed and sorted for determinism. A missing `dest` is not
/// an error — an empty manifest is written and Ok returned.
pub fn write_checksums(dir: &Path, dest: &Path) -> io::Result<()> {
    let d = reports_dir(dir)?;
    let path = d.join("checksums.sha256");

    if !dest.exists() {
        fs::write(&path, b"")?;
        return Ok(());
    }

    let mut rels: Vec<String> = Vec::new();
    for entry in walkdir::WalkDir::new(dest)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_dir() {
            continue;
        }
        let rel = match entry.path().strip_prefix(dest) {
            Ok(r) => r,
            Err(_) => continue,
        };
        // forward-slash the relative path (Go filepath.ToSlash).
        let slashed = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        rels.push(slashed);
    }
    // sort.Strings -> byte-wise lexicographic order.
    rels.sort();

    let mut sb = String::new();
    for rel in &rels {
        let abs = dest.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        match checksum_file(&abs) {
            Ok(sum) => sb.push_str(&format!("{}  {}\n", hex::encode(sum), rel)),
            Err(_) => continue, // unreadable file: skip rather than fail.
        }
    }
    fs::write(&path, sb.as_bytes())
}

fn checksum_file(path: &Path) -> io::Result<[u8; 32]> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    io::copy(&mut f, &mut h)?;
    Ok(h.finalize().into())
}

/// WriteManifest emits the human-facing MANIFEST.md summary.
pub fn write_manifest(dir: &Path, p: &model::Plan, res: &model::ApplyResult) -> io::Result<()> {
    let d = reports_dir(dir)?;
    let mut sb = String::new();
    sb.push_str(&format!(
        "# Consolidation manifest\n\nGenerated: {}\nRoots: {}\nDest: {}\n\n",
        p.generated_at,
        go_slice_v(&p.roots),
        p.dest
    ));
    sb.push_str(&format!(
        "Copied: {}  Quarantined: {}  Unioned: {}  Skipped: {}\n\n",
        res.copied, res.quarantined, res.unioned, res.skipped
    ));
    sb.push_str("| owner | repo | strategy | canonical | quarantined | redundant |\n");
    sb.push_str("|---|---|---|---|---|---|\n");
    for dec in &p.decisions {
        sb.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            dec.group.owner,
            dec.group.repo_name,
            strategy_str(dec.strategy),
            dec.canonical.machine,
            dec.quarantine.len(),
            dec.redundant.len()
        ));
    }
    let n = res.skipped_files.len();
    if n > 0 {
        sb.push_str(&format!("\n## Unreadable files skipped ({})\n\n", n));
        let limit = if n > 50 { 50 } else { n };
        for f in &res.skipped_files[..limit] {
            sb.push_str(&format!("- {}\n", f));
        }
        if n > 50 {
            sb.push_str(&format!("- ... and {} more\n", n - 50));
        }
    }
    let mut file = fs::File::create(d.join("MANIFEST.md"))?;
    file.write_all(sb.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// A unique scratch dir under the system temp dir (Go `t.TempDir`).
    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> TempDir {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let p = std::env::temp_dir().join(format!("reposmerge-report-{nanos}-{n}"));
            fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sha256_hex(data: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(data);
        hex::encode(h.finalize())
    }

    // Port of Go TestWriteAndLoadPlanRoundTrips.
    #[test]
    fn test_write_and_load_plan_round_trips() {
        let dir = TempDir::new();
        let p = model::Plan {
            roots: vec!["D:/weaver-sync".to_string()],
            dest: "canonical".to_string(),
            generated_at: "2026-06-24T00:00:00Z".to_string(),
            decisions: vec![model::Decision {
                group: model::Group {
                    repo_name: "omni".to_string(),
                    owner: "inovacc".to_string(),
                    has_remote: true,
                    ..Default::default()
                },
                strategy: model::StrategyKind::A,
                dest_path: "canonical/inovacc/omni".to_string(),
                canonical: model::Copy {
                    machine: "live".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        };
        write_plan(dir.path(), &p).unwrap();
        assert!(
            dir.path().join("reports").join("divergence.md").exists(),
            "divergence.md missing"
        );
        let got = load_plan(&dir.path().join("reports").join("plan.json")).unwrap();
        assert_eq!(got.decisions.len(), 1);
        assert_eq!(got.decisions[0].group.repo_name, "omni");
    }

    // Port of Go TestWriteChecksumsFormatAndDeterminism.
    #[test]
    fn test_write_checksums_format_and_determinism() {
        let dir = TempDir::new();
        let dest = TempDir::new();
        fs::write(dest.path().join("b.txt"), b"bbb").unwrap();
        fs::create_dir_all(dest.path().join("sub")).unwrap();
        fs::write(dest.path().join("sub").join("a.txt"), b"aaa").unwrap();

        write_checksums(dir.path(), dest.path()).unwrap();
        let b = fs::read_to_string(dir.path().join("reports").join("checksums.sha256")).unwrap();
        let lines: Vec<&str> = b.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 2, "expected 2 lines, got: {b:?}");
        let mut sorted = lines.clone();
        sorted.sort();
        assert_eq!(lines, sorted, "expected sorted lines: {lines:?}");
        let want_b = format!("{}  b.txt", sha256_hex(b"bbb"));
        let want_a = format!("{}  sub/a.txt", sha256_hex(b"aaa"));
        assert!(lines.contains(&want_b.as_str()), "missing line: {want_b}");
        assert!(lines.contains(&want_a.as_str()), "missing line: {want_a}");
    }

    // Port of Go TestWriteInventoryWritesCSVs.
    #[test]
    fn test_write_inventory_writes_csvs() {
        let dir = TempDir::new();
        let in_scope = vec![model::Copy {
            owner: "inovacc".to_string(),
            repo_name: "omni".to_string(),
            machine: "live".to_string(),
            remote_url: "https://github.com/inovacc/omni.git".to_string(),
            path: "repos/root1/inovacc/omni".to_string(),
            fp: model::Fingerprint {
                head: "abc123".to_string(),
                commit_count: 3,
                ahead: 1,
                dirty_count: 0,
                untracked_count: 2,
                ..Default::default()
            },
            ..Default::default()
        }];
        let third_party = vec![model::Copy {
            owner: String::new(),
            repo_name: "thirdlib".to_string(),
            machine: "live".to_string(),
            path: "repos/root1/vendor/thirdlib".to_string(),
            ..Default::default()
        }];
        write_inventory(dir.path(), &in_scope, &third_party).unwrap();
        let inv = fs::read_to_string(dir.path().join("reports").join("inventory.csv")).unwrap();
        assert!(inv.starts_with("owner,repo,machine"), "bad header: {inv:?}");
        assert!(inv.contains("inovacc,omni,live"), "missing row: {inv:?}");
        let tp = fs::read_to_string(dir.path().join("reports").join("third-party.csv")).unwrap();
        assert!(tp.contains("thirdlib"), "missing row: {tp:?}");
    }

    // Port of Go TestWriteChecksumsMissingDestNoError.
    #[test]
    fn test_write_checksums_missing_dest_no_error() {
        let dir = TempDir::new();
        let missing = TempDir::new();
        let missing_path = missing.path().join("does-not-exist");
        write_checksums(dir.path(), &missing_path).expect("no error for missing dest");
        assert!(
            dir.path().join("reports").join("checksums.sha256").exists(),
            "checksums.sha256 should still be written"
        );
    }

    fn golden_plan() -> model::Plan {
        model::Plan {
            roots: vec!["repos/root1".to_string(), "repos/root2".to_string()],
            dest: "canonical".to_string(),
            generated_at: "2026-06-24T00:00:00Z".to_string(),
            decisions: vec![
                model::Decision {
                    group: model::Group {
                        key: "https://github.com/inovacc/omni.git".to_string(),
                        owner: "inovacc".to_string(),
                        repo_name: "omni".to_string(),
                        has_remote: true,
                        remote_url: "https://github.com/inovacc/omni.git".to_string(),
                        copies: vec![
                            model::Copy {
                                path: "repos/root1/inovacc/omni".to_string(),
                                root: "repos/root1".to_string(),
                                machine: "live".to_string(),
                                owner: "inovacc".to_string(),
                                repo_name: "omni".to_string(),
                                remote_url: "https://github.com/inovacc/omni.git".to_string(),
                                fp: model::Fingerprint {
                                    head: "aaaaaaa1111111111111111111111111111111".to_string(),
                                    all_commits: vec![
                                        "aaaaaaa1111111111111111111111111111111".to_string()
                                    ],
                                    commit_count: 1,
                                    ahead: 2,
                                    ..Default::default()
                                },
                            },
                            model::Copy {
                                path: "repos/root2/inovacc/omni".to_string(),
                                root: "repos/root2".to_string(),
                                machine: "acer".to_string(),
                                owner: "inovacc".to_string(),
                                repo_name: "omni".to_string(),
                                remote_url: "https://github.com/inovacc/omni.git".to_string(),
                                fp: model::Fingerprint {
                                    head: "bbbbbbb2222222222222222222222222222222".to_string(),
                                    dirty_count: 1,
                                    ..Default::default()
                                },
                            },
                        ],
                    },
                    strategy: model::StrategyKind::A,
                    canonical: model::Copy {
                        path: "repos/root1/inovacc/omni".to_string(),
                        root: "repos/root1".to_string(),
                        machine: "live".to_string(),
                        owner: "inovacc".to_string(),
                        repo_name: "omni".to_string(),
                        remote_url: "https://github.com/inovacc/omni.git".to_string(),
                        fp: model::Fingerprint {
                            head: "aaaaaaa1111111111111111111111111111111".to_string(),
                            ahead: 2,
                            ..Default::default()
                        },
                    },
                    canonical_reason: "most commits ahead of origin".to_string(),
                    dest_path: "canonical/inovacc/omni".to_string(),
                    quarantine: vec![model::QuarantineItem {
                        copy: model::Copy {
                            path: "repos/root2/inovacc/omni".to_string(),
                            root: "repos/root2".to_string(),
                            machine: "acer".to_string(),
                            owner: "inovacc".to_string(),
                            repo_name: "omni".to_string(),
                            ..Default::default()
                        },
                        dest_path: "canonical/inovacc/omni/_quarantine/acer".to_string(),
                        reason: "dirty working tree with unreachable commits".to_string(),
                        unreachable_commits: vec![
                            "deadbeef1111111111111111111111111111111".to_string()
                        ],
                    }],
                    redundant: vec!["repos/root3/inovacc/omni".to_string()],
                    union_remotes: Vec::new(),
                },
                model::Decision {
                    group: model::Group {
                        key: "noremote:root1234567:loom".to_string(),
                        owner: "personal".to_string(),
                        repo_name: "loom".to_string(),
                        has_remote: false,
                        remote_url: String::new(),
                        copies: vec![
                            model::Copy {
                                path: "repos/root1/personal/loom".to_string(),
                                root: "repos/root1".to_string(),
                                machine: "live".to_string(),
                                owner: "personal".to_string(),
                                repo_name: "loom".to_string(),
                                fp: model::Fingerprint {
                                    root_commits: vec![
                                        "root1234567890123456789012345678901234".to_string()
                                    ],
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                            model::Copy {
                                path: "repos/root2/personal/loom".to_string(),
                                root: "repos/root2".to_string(),
                                machine: "acer".to_string(),
                                owner: "personal".to_string(),
                                repo_name: "loom".to_string(),
                                fp: model::Fingerprint {
                                    root_commits: vec![
                                        "root1234567890123456789012345678901234".to_string()
                                    ],
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                        ],
                    },
                    strategy: model::StrategyKind::B,
                    canonical: model::Copy {
                        path: "repos/root1/personal/loom".to_string(),
                        root: "repos/root1".to_string(),
                        machine: "live".to_string(),
                        owner: "personal".to_string(),
                        repo_name: "loom".to_string(),
                        ..Default::default()
                    },
                    canonical_reason: "richest fingerprint (shared lineage)".to_string(),
                    dest_path: "canonical/personal/loom".to_string(),
                    union_remotes: vec![model::UnionRemote {
                        name: "consolidate-acer".to_string(),
                        path: "repos/root2/personal/loom".to_string(),
                        branches: vec!["main".to_string(), "feature-x".to_string()],
                    }],
                    ..Default::default()
                },
            ],
            third_party: vec![model::Copy {
                path: "repos/root1/vendor/thirdlib".to_string(),
                root: "repos/root1".to_string(),
                machine: "live".to_string(),
                repo_name: "thirdlib".to_string(),
                ..Default::default()
            }],
        }
    }

    fn golden_apply_result() -> model::ApplyResult {
        model::ApplyResult {
            copied: 2,
            quarantined: 1,
            unioned: 1,
            skipped: 0,
            skipped_files: vec!["repos/root1/inovacc/omni/locked.txt".to_string()],
            ..Default::default()
        }
    }

    fn compare_golden(got_path: &Path, golden_name: &str) {
        let got = fs::read(got_path).unwrap();
        let golden_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("golden")
            .join(golden_name);
        let want = fs::read(&golden_path).unwrap();
        if got != want {
            let gl: Vec<&[u8]> = got.split(|&b| b == b'\n').collect();
            let wl: Vec<&[u8]> = want.split(|&b| b == b'\n').collect();
            let mut first = gl.len().max(wl.len());
            for i in 0..gl.len().min(wl.len()) {
                if gl[i] != wl[i] {
                    first = i + 1;
                    break;
                }
            }
            panic!(
                "{} does not match golden {} (first differing line {})\n--- got ---\n{}\n--- want ---\n{}",
                got_path.display(),
                golden_path.display(),
                first,
                String::from_utf8_lossy(&got),
                String::from_utf8_lossy(&want),
            );
        }
    }

    // Port of Go TestGoldenReportOutputs.
    #[test]
    fn test_golden_report_outputs() {
        let dir = TempDir::new();
        let p = golden_plan();
        let res = golden_apply_result();

        write_plan(dir.path(), &p).unwrap();
        write_manifest(dir.path(), &p, &res).unwrap();

        let reports = dir.path().join("reports");
        compare_golden(&reports.join("plan.json"), "plan.json");
        compare_golden(&reports.join("divergence.md"), "divergence.md");
        compare_golden(&reports.join("MANIFEST.md"), "MANIFEST.md");
    }
}
