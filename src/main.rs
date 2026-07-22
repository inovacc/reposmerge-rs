//! reposmerge CLI entry point — faithful port of `cmd/reposmerge` (Go cobra).
//!
//! ## Framework boundary (mantle)
//! Go's `main.go` calls `bootstrap.Configure(root, app.New(), ...)`, wiring
//! mantle's runtime (viper config load, otel observability, structured logger,
//! daemon supervisor) into cobra's `PersistentPreRunE`. reposmerge's own
//! commands never read that runtime, so it is NOT reimplemented (see `app.rs`).
//! We reproduce only the OBSERVABLE CLI surface: the four subcommands with exact
//! flag names/defaults, a global `--config/-c` flag (accepted, unused, for
//! parity), and `--version`. The other mantle global flags (`--env`,
//! `--log-level`, `--verbose`, `--quiet`, `--log-format`, `--log-source`,
//! `--no-redact`, `--otel*`, `--daemon`) are intentionally OMITTED as
//! out-of-scope framework plumbing.
//!
//! Go cobra → Rust clap (derive). Cobra `SilenceUsage=true` → on error we print
//! just the error to stderr and exit(1) (no usage dump).

use std::error::Error;
use std::path::Path;

use chrono::{DateTime, Local, SecondsFormat, Utc};
use clap::{Parser, Subcommand};

use reposmerge::consolidate::{self, Options};
use reposmerge::discover::{default_scope, discover};
use reposmerge::fingerprint;
use reposmerge::gitx::new_runner;
use reposmerge::group;
use reposmerge::model::{Decision, Plan};
use reposmerge::report;
use reposmerge::safety::{physical_reachability, reachability_proof};
use reposmerge::strategy;

/// reposmerge — consolidate scattered git repo copies into one canonical tree.
#[derive(Parser)]
#[command(
    name = "reposmerge",
    version,
    about = "reposmerge — consolidate scattered git repo copies into one canonical tree"
)]
struct Cli {
    /// config file (accepted for parity with the mantle framework; unused).
    #[arg(short = 'c', long, global = true)]
    config: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Discover, fingerprint, and group repo copies (read-only; no writes to dest)
    Scan {
        /// root dir(s) to scan (repeatable or comma-separated)
        #[arg(long, required = true, value_delimiter = ',')]
        roots: Vec<String>,
        /// directory to write reports/ into
        #[arg(long, default_value = ".")]
        out: String,
        /// planned canonical dest (recorded in the plan)
        #[arg(long, default_value = "./canonical")]
        dest: String,
        /// parallel git workers
        #[arg(long, default_value_t = default_workers())]
        workers: usize,
        /// also discover repos nested inside another repo's tree
        #[arg(long)]
        include_nested: bool,
    },
    /// Decide A/B/C strategy per group and write the action plan (no writes to dest)
    Plan {
        /// directory containing reports/ (read + rewrite)
        #[arg(long, default_value = ".")]
        out: String,
        /// override canonical dest (default: value from scan)
        #[arg(long, default_value = "")]
        dest: String,
    },
    /// Execute the consolidation plan (dry-run unless --confirm)
    Apply {
        /// path to plan.json
        #[arg(long, default_value = "reports/plan.json")]
        plan: String,
        /// canonical output dir
        #[arg(long, default_value = "./canonical")]
        dest: String,
        /// dir for reports/
        #[arg(long, default_value = ".")]
        out: String,
        /// actually write (default is dry-run)
        #[arg(long)]
        confirm: bool,
        /// include node_modules/vendor/etc
        #[arg(long)]
        include_generated: bool,
    },
    /// Prove no source commit was lost (static plan proof + optional physical check)
    Verify {
        /// path to plan.json
        #[arg(long, default_value = "reports/plan.json")]
        plan: String,
        /// also query the real consolidated repos (run after apply --confirm)
        #[arg(long)]
        physical: bool,
    },
}

/// Go `defaultWorkers()`: NumCPU*2, capped 16, min 1.
fn default_workers() -> usize {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    // Go: NumCPU*2, then `if n>16 {16}; if n<1 {1}` — equivalent to clamp(1, 16).
    (cpus * 2).clamp(1, 16)
}

fn run_scan(
    roots: Vec<String>,
    out: String,
    dest: String,
    mut workers: usize,
    include_nested: bool,
) -> Result<(), Box<dyn Error>> {
    let (mut in_scope, third_party) = discover(
        &roots,
        &default_scope(),
        &consolidate::default_excludes(),
        include_nested,
    )?;
    if workers < 1 {
        workers = 1;
    }
    // Bounded worker pool: split the in-scope slice into <=workers disjoint
    // contiguous chunks and fingerprint each in its own thread. Each index is
    // written by exactly one thread (safe). Go used a semaphore over goroutines
    // mutating inScope[i]; std::thread::scope needs no new dependency.
    let n = in_scope.len();
    if n > 0 {
        let runner = new_runner();
        let chunk = n.div_ceil(workers).max(1);
        std::thread::scope(|s| {
            for slice in in_scope.chunks_mut(chunk) {
                let runner = &runner;
                s.spawn(move || {
                    for copy in slice.iter_mut() {
                        let _ = fingerprint::compute(runner, copy);
                        if let Ok(md) = std::fs::metadata(&copy.path) {
                            if let Ok(mt) = md.modified() {
                                // Go fi.ModTime() is local time with its offset;
                                // match that rather than normalizing to UTC.
                                copy.fp.dir_mtime = DateTime::<Local>::from(mt).fixed_offset();
                            }
                        }
                    }
                });
            }
        });
    }

    let unreadable = in_scope
        .iter()
        .filter(|c| c.fp.head.is_empty() && c.fp.commit_count == 0)
        .count();

    let groups = group::build(in_scope.clone());
    let mut plan = Plan {
        roots: roots.clone(),
        dest: dest.clone(),
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        third_party: third_party.clone(),
        ..Default::default()
    };
    for g in &groups {
        plan.decisions.push(Decision {
            group: g.clone(),
            ..Default::default()
        });
    }

    let out_path = Path::new(&out);
    report::write_inventory(out_path, &in_scope, &third_party)?;
    report::write_plan(out_path, &plan)?;
    println!(
        "scanned: {} in-scope copies, {} third-party, {} groups ({} unreadable) -> {}",
        in_scope.len(),
        third_party.len(),
        groups.len(),
        unreadable,
        Path::new(&out).join("reports").display()
    );
    Ok(())
}

fn run_plan(out: String, dest: String) -> Result<(), Box<dyn Error>> {
    let plan_path = Path::new(&out).join("reports").join("plan.json");
    let mut pl = report::load_plan(&plan_path)?;
    if !dest.is_empty() {
        pl.dest = dest;
    }
    let decided: Vec<Decision> = pl
        .decisions
        .iter()
        .map(|d| strategy::decide(&d.group, &pl.dest))
        .collect();
    let count = decided.len();
    pl.decisions = decided;
    report::write_plan(Path::new(&out), &pl)?;
    println!("planned {} group(s) -> {}", count, plan_path.display());
    Ok(())
}

fn run_apply(
    plan: String,
    dest: String,
    out: String,
    confirm: bool,
    include_generated: bool,
) -> Result<(), Box<dyn Error>> {
    let p = report::load_plan(Path::new(&plan))?;
    let vio = reachability_proof(&p);
    if !vio.is_empty() {
        for v in &vio {
            eprintln!(
                "LOSS RISK: {}/{} commit {} unaccounted for",
                v.repo, v.machine, v.sha
            );
        }
        return Err(format!(
            "aborting: {} commits would be lost; fix plan before applying",
            vio.len()
        )
        .into());
    }
    let res = consolidate::apply(
        &new_runner(),
        &p,
        &Options {
            dest: dest.clone(),
            dry_run: !confirm,
            include_generated,
            exclude_dirs: None,
        },
    )?;
    if confirm {
        let pv = physical_reachability(&new_runner(), &p);
        if !pv.is_empty() {
            for v in &pv {
                eprintln!(
                    "POST-APPLY LOSS: {}/{} commit {} not in consolidated repo",
                    v.repo, v.machine, v.sha
                );
            }
            return Err(format!(
                "post-apply verification FAILED: {} commit(s) missing after union",
                pv.len()
            )
            .into());
        }
        println!("post-apply physical verification OK");
    }
    if !confirm {
        println!("DRY-RUN (no files written). Re-run with --confirm to execute.");
    }
    println!(
        "copied={} quarantined={} unioned={}",
        res.copied, res.quarantined, res.unioned
    );
    if !res.skipped_files.is_empty() {
        eprintln!(
            "WARNING: {} file(s) unreadable and skipped (locked/permission-denied); see reports/MANIFEST.md",
            res.skipped_files.len()
        );
    }
    report::write_manifest(Path::new(&out), &p, &res)
        .map_err(|e| format!("write manifest: {e}"))?;
    if confirm {
        report::write_checksums(Path::new(&out), Path::new(&dest))
            .map_err(|e| format!("write checksums: {e}"))?;
    }
    Ok(())
}

fn run_verify(plan: String, physical: bool) -> Result<(), Box<dyn Error>> {
    let pl = report::load_plan(Path::new(&plan))?;
    let mut vio = reachability_proof(&pl);
    if physical {
        vio.extend(physical_reachability(&new_runner(), &pl));
    }
    if !vio.is_empty() {
        for v in &vio {
            eprintln!(
                "LOSS: {}/{} commit {} unaccounted for",
                v.repo, v.machine, v.sha
            );
        }
        return Err(format!("verify FAILED: {} commit(s) unaccounted for", vio.len()).into());
    }
    println!(
        "verify OK: every source commit is accounted for across {} group(s)",
        pl.decisions.len()
    );
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Scan {
            roots,
            out,
            dest,
            workers,
            include_nested,
        } => run_scan(roots, out, dest, workers, include_nested),
        Commands::Plan { out, dest } => run_plan(out, dest),
        Commands::Apply {
            plan,
            dest,
            out,
            confirm,
            include_generated,
        } => run_apply(plan, dest, out, confirm, include_generated),
        Commands::Verify { plan, physical } => run_verify(plan, physical),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use reposmerge::model::{Copy, Decision, Fingerprint, Group, Plan, StrategyKind};
    use std::fs;

    // Faithful port of Go TestSubcommandsRegistered.
    #[test]
    fn subcommands_registered() {
        let cmd = Cli::command();
        let names: Vec<&str> = cmd.get_subcommands().map(|c| c.get_name()).collect();
        for want in ["scan", "plan", "apply", "verify"] {
            assert!(names.contains(&want), "subcommand {want:?} not registered");
        }
    }

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("reposmerge-cli-{tag}"));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_plan_json(dir: &std::path::Path, p: &Plan) -> String {
        report::write_plan(dir, p).unwrap();
        dir.join("reports")
            .join("plan.json")
            .to_string_lossy()
            .into_owned()
    }

    fn fp(commits: &[&str]) -> Fingerprint {
        Fingerprint {
            all_commits: commits.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    // run_verify OK path: a plan with no unaccounted commits verifies clean.
    #[test]
    fn run_verify_ok_when_no_loss() {
        let dir = tmp("verify-ok");
        let plan = Plan {
            dest: "canonical".into(),
            ..Default::default()
        };
        let plan_path = write_plan_json(&dir, &plan);
        assert!(run_verify(plan_path, false).is_ok());
    }

    // run_verify loss path: a divergent copy with an unreachable commit fails.
    #[test]
    fn run_verify_fails_on_commit_loss() {
        let dir = tmp("verify-loss");
        let canon = Copy {
            path: "/live".into(),
            machine: "live".into(),
            fp: fp(&["a"]),
            ..Default::default()
        };
        let other = Copy {
            path: "/acer".into(),
            machine: "acer".into(),
            fp: fp(&["a", "z"]), // 'z' unreachable in canonical
            ..Default::default()
        };
        let plan = Plan {
            decisions: vec![Decision {
                strategy: StrategyKind::A,
                group: Group {
                    repo_name: "x".into(),
                    copies: vec![canon.clone(), other],
                    ..Default::default()
                },
                canonical: canon,
                ..Default::default()
            }],
            ..Default::default()
        };
        let plan_path = write_plan_json(&dir, &plan);
        assert!(run_verify(plan_path, false).is_err());
    }

    // run_apply dry-run: reports a planned copy and writes nothing under dest.
    #[test]
    fn run_apply_dry_run_writes_nothing() {
        let dir = tmp("apply-dry");
        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("f.txt"), "x").unwrap();
        let dest = dir.join("canonical");
        let plan = Plan {
            decisions: vec![Decision {
                strategy: StrategyKind::A,
                canonical: Copy {
                    path: src.to_string_lossy().into_owned(),
                    machine: "live".into(),
                    ..Default::default()
                },
                dest_path: dest
                    .join("inovacc")
                    .join("omni")
                    .to_string_lossy()
                    .into_owned(),
                group: Group {
                    repo_name: "omni".into(),
                    owner: "inovacc".into(),
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        };
        let plan_path = write_plan_json(&dir, &plan);
        // confirm=false => dry-run: Ok, and dest/ must not be created.
        run_apply(
            plan_path,
            dest.to_string_lossy().into_owned(),
            dir.to_string_lossy().into_owned(),
            false,
            false,
        )
        .unwrap();
        assert!(!dest.exists(), "dry-run must not create dest");
    }
}
