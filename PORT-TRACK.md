# PORT-TRACK — reposmerge Go → Rust

Source: `github.com/inovacc/reposmerge` @ `479a7c585fad56d4330a1136e5f19b682d02c609`
Pair: go2rust · Scope: full 1:1 parity · Target: `../reposmerge-rs`

## Audit (differential Go-oracle) — 3 findings fixed → byte-parity
`/unravel:port:audit` ran the real CLI (scan/plan/verify) through BOTH the Go and
Rust binaries against an identical git-repo tree and diffed outputs. All 4 report
artifacts (plan.json/inventory.csv/third-party.csv/divergence.md) are now
BYTE-IDENTICAL (plan.json 7904 bytes). Fixes applied:
1. **DestPath `./` (strategy::path_join)** — CLI default `dest_root="./canonical"`;
   Go `path.Join` runs `path.Clean` (→ `canonical/...`), the Rust join did not
   (→ `./canonical/...`). Fixed: `path_join` now ports Go `path.Clean` faithfully.
2. **Timestamp timezone (model/fingerprint/main)** — Go keeps the commit's local
   offset (`%cI`) and the file's local mtime offset; the port normalized to UTC
   (`Z`). Fixed: `LastCommit`/`DirMtime` are now `DateTime<FixedOffset>`; the offset
   is preserved (zero offset still emits `Z`, matching Go's zero-time + UTC commits).
3. **Fractional seconds (model::go_time)** — Go RFC3339Nano trims trailing zeros
   (`.2496821`); chrono `AutoSi` group-quantized (`.249682100`). Fixed: hand-rolled
   nanosecond formatting trims trailing zeros.
Remaining stdout difference is ONLY mantle's slog framework logging (out-of-scope
boundary, see below); reposmerge's own stdout lines match byte-for-byte.

## Modules (dependency order) — 11 port-units

| # | module | tests-ported | code-ported | verified | parity | commit |
|---|--------|:---:|:---:|:---:|--------|--------|
| 1 | model       | ☑ | ☑ | ☑ | PASS (1 test) | (see git) |
| 2 | gitx        | ☑ | ☑ | ☑ | PASS (2 tests total) | (see git) |
| 3 | fingerprint | ☑ | ☑ | ☑ | PASS (3 tests total) | (see git) |
| 4 | group       | ☑ | ☑ | ☑ | PASS (6 tests total) | (see git) |
| 5 | discover    | ☑ | ☑ | ☑ | PASS (13 tests total) | (see git) |
| 6 | report      | ☑ | ☑ | ☑ | PASS (18 tests, byte-golden ✓) | (see git) |
| 7 | safety      | ☑ | ☑ | ☑ | PASS (32 tests total, Win rollback ✓) | (see git) |
| 8 | strategy    | ☑ | ☑ | ☑ | PASS (38 tests total) | (see git) |
| 9 | consolidate | ☑ | ☑ | ☑ | PASS (49 tests total, real-git ✓) | (see git) |
| 10| app         | ☑ | ☑ | ☑ | PASS (inert shim; mantle runtime out-of-scope) | (see git) |
| 11| cmd (main)  | ☑ | ☑ | ☑ | PASS (cli subcommands test; binary builds) | (see git) |
|  + | e2e (tests/) | ☑ | ☑ | ☑ | PASS (real-git full-pipeline ✓) | (see git) |

## Dependencies added
- `serde` (+ derive) — JSON (de)serialization parity for `report` module; std has
  no serialization. Alt considered: hand-rolled — rejected (reinvention defect).
- `chrono` (features=["serde"]) — RFC3339/calendar time parity for `LastCommit`/
  `DirMtime`; `std::time` cannot format calendar dates or the Go zero-time
  `0001-01-01T00:00:00Z`. Alt: `time` crate — chrono chosen for serde+RFC3339Nano.
- `serde_json` — added at `report` module (JSON MarshalIndent/Unmarshal parity).
- `csv` (module 6, report) — Go `encoding/csv`; std has no CSV writer. Alt: hand-roll
  quoting — rejected (edge cases: embedded quotes/commas/newlines). Terminator forced
  to LF to match Go default (crate default is CRLF).
- `sha2` (module 5, discover) — Sha256 for `source_disc`; std has no crypto. Alt:
  hand-rolled sha256 = reinvention defect, rejected.
- `hex` (module 5, discover) — hex-encode the digest (Go encoding/hex). Well-known;
  alt: hand-roll ~3 lines — chose `hex` for clarity, logged.
- `walkdir` (module 5, discover) — faithful recursive walk with prune/skip-subtree
  matching Go `filepath.WalkDir` + `filepath.SkipDir`; `std::fs` has no walker with
  skip-descend control. Uses `into_iter()` + `it.skip_current_dir()` to mirror SkipDir.

## gitx (module 2)
- Dependencies added: **none** (std only: std::process, std::collections,
  std::path, std::cell, std::fmt).
- `Runner` trait drops Go's `context.Context` param (no cancellation tested).
  `run(&self, dir: &str, args: &[&str]) -> Result<String, GitError>` — takes
  `&self` so `&dyn Runner` works; `Fake` uses `RefCell<Vec<String>>` for `calls`.
- Error type `GitError` (struct) reproduces Go string
  `git <args> (in <dir>): <cause>: <trimmed stderr>` via `Display`. `cause` for
  exit failures is `exit status N` (mirrors Go `*exec.ExitError`); PARITY note:
  exact spawn-error text (git-not-found) is OS-specific and untested.
- NOT exec-verified: porter had no Bash/exec. Conductor must run `cargo test`
  (fail→green — though the single Fake test may pass immediately since it needs
  no ExecRunner; still confirm build), `cargo build`, fill provenance sha256,
  commit.

## fingerprint (module 3)
- Dependencies added: **none** (uses existing `chrono` for RFC3339 parse, plus
  `crate::gitx`/`crate::model`).
- `compute(r: &dyn Runner, c: &mut Copy) -> Result<(), GitError>` — Go `Compute`
  with `context.Context` DROPPED. Fills `c.fp`. Helpers `safe` (=
  `Result::unwrap_or_default`) and `lines` (trim whole string; "" → empty vec;
  else split on '\n').
- Faithful details preserved: `head` keeps "" on error (`unwrap_or_default`);
  `safe` swallows git errors → "" → empty lines; `root_commits`/`all_commits`
  sorted with `Vec::sort` (byte/lexicographic = Go `sort.Strings`); branch split
  on FIRST ' ' (`split_once`), skip lines with no space; status "??" prefix →
  untracked, else non-blank → dirty; ahead/behind split on FIRST '\t', atoi
  failure → 0 (`parse().unwrap_or(0)`), behind=left/ahead=right; last_commit via
  `DateTime::parse_from_rfc3339(...).with_timezone(&Utc)`, parse error leaves
  zero-time.
- PARITY concerns:
  - RFC3339 tz: Go `time.Parse(RFC3339, "…-03:00")` keeps the offset instant;
    Rust converts to UTC (`with_timezone(&Utc)`). The instant is identical; only
    the stored zone differs. Since `Fingerprint::last_commit` is `DateTime<Utc>`
    (per model port) this is the intended representation; JSON RFC3339Nano output
    will render in UTC (e.g. `2026-06-20T13:00:00Z`) vs Go which would re-emit the
    original offset. VERIFY against a Go golden at `report`; may need offset
    preservation if byte-parity required.
  - `sort()` on `Vec<String>` is byte-lexicographic = Go `sort.Strings`. OK.
  - atoi edge: Go `strconv.Atoi` accepts leading/trailing per `TrimSpace`; Rust
    `.trim().parse::<i64>()` matches (both reject "+"/non-digit → 0). OK.
- NOT exec-verified: porter had no Bash/exec. Conductor must run `cargo test`
  (fail→green — note the single Fake test likely passes on first compile since it
  needs no ExecRunner; confirm build + the fail state came only from the missing
  module), `cargo build`, fill provenance sha256, commit.

## group (module 4)
- Dependencies added: **none** (std only: `std::collections::HashMap`; consumes
  `crate::model::{Copy, Group}`).
- `build(copies: Vec<Copy>) -> Vec<Group>` (Go `Build([]Copy) []Group`).
- FAITHFUL insertion order: Go keeps `order []string` beside `map[string]*Group`
  and emits groups in first-seen key order. Reproduced with `order: Vec<String>`
  + `HashMap<String, Group>`; `or_insert_with` pushes the key to `order` only on
  first sight. HashMap iteration order is NEVER used → deterministic parity.
- `group_key`: `remote:<url>` when `remote_url` non-empty, else
  `noremote:<repo_name>:<lineage>`. `lineage`: clone `fp.root_commits`, `sort()`
  (byte-lexicographic == Go `sort.Strings`); empty → `"EMPTY"`, else join `,`.
- Tests: 3 faithful ports (remote merge / divergent-lineage no-merge / same-
  lineage merge) with `cp(name,url,root)` helper.
- NOT exec-verified: porter had no Bash/exec. Conductor must run `cargo test`
  (confirm the 3 group tests fail before compile / green after — since the module
  was gated behind `// pub mod group;`, the fail state is the un-compiled module),
  `cargo build`, fill provenance sha256 (currently `PENDING-EXEC`), commit.

## discover (module 5)
- Dependencies added: `sha2`, `hex`, `walkdir` (justified above). Consumes
  `crate::model::Copy` and `crate::gitx::{new_runner, is_repo, Runner, GitError}`.
- Public API: `Scope{in_scope_owners, third_party_dirs}`, `default_scope() -> Scope`,
  `normalize_url(&str) -> String`, `parse_owner_repo(&str,&str) -> (String,String)`,
  `infer_machine(&str) -> String`, `source_disc(&str) -> String`,
  `discover(&[String], &Scope, &[String], bool) -> Result<(Vec<Copy>,Vec<Copy>),GitError>`,
  `pub(crate) is_third_party(&str,&str,&Scope) -> bool`. `Discover` drops Go ctx.
- FAITHFUL details: `normalize_url` reproduces the Go trim sequence byte-for-byte
  incl. the `u+"/"` first-slash trick via `find('/').unwrap_or(len)`; single
  TrimSuffix/TrimPrefix via `strip_*` fallbacks; `Replace(u,":","/",1)` via
  `replace_range` on first ':'. `infer_machine` does ToSlash FIRST then lowercase,
  switch order preserved. `to_slash` is platform-aware (no-op on Unix where SkipDir
  test paths already use '/').
- dir-extraction decision (source_disc): Go uses `filepath.ToSlash(filepath.Dir(p))`.
  Implemented as: normalize to '/', strip one trailing '/', cut at last '/'. Passes
  determinism + differs-by-path. Documented in code comment.
- walk: `WalkDir::new(root).into_iter()` loop; tolerate `Err` entries (Go returns nil
  on error → continue); dirs only; exclude-dir names and ".git" → `skip_current_dir()`;
  non-repo → continue; repo found → build Copy, classify, and when `!include_nested`
  call `skip_current_dir()` (== Go SkipDir). ".git" is skipped by NAME before is_repo
  so it is never returned as a repo.
- PARITY concerns:
  - walkdir `skip_current_dir()` vs `filepath.SkipDir`: both prune the current
    directory's remaining subtree. Semantics match for the tested cases; note walkdir
    prunes children not-yet-yielded of the current dir, which is the SkipDir contract.
  - `infer_machine` ToSlash-then-lowercase ordering preserved (order only matters if a
    separator were uppercase-sensitive — it isn't — but kept faithful regardless).
  - `test_discover_nested_repos` EXECs real `git config` (returns error on bare .git
    dirs → url "") and touches FS under system temp; robust to git errors via
    `unwrap_or_default()`. Conductor must have `git` on PATH.
- NOT exec-verified: porter had no Bash/exec. Conductor must run `cargo test`
  (fail→green: module was gated behind `// pub mod discover;`), `cargo build`, fill
  provenance sha256 (currently `PENDING-EXEC`), commit.

## report (module 6) — HIGHEST parity risk (byte-exact goldens)
- Dependencies added: `serde_json`, `csv` (justified above). Reuses `sha2`, `hex`,
  `walkdir`. Consumes `crate::model::*`.
- Public API (all take `&Path`, return `io::Result`):
  `write_inventory(dir, in_scope: &[Copy], third_party: &[Copy])`,
  `write_plan(dir, p: &Plan)`, `load_plan(path) -> io::Result<Plan>`,
  `write_checksums(dir, dest)`, `write_manifest(dir, p: &Plan, res: &ApplyResult)`.
  Private: `reports_dir`, `write_csv`, `checksum_file`, `go_slice_v`, `strategy_str`.
- **MODEL CHANGE (parity fix 1) — nil-slice → JSON `null`.** Added `null_if_empty`
  serde helper module in `src/model.rs`; applied `serialize_with`/`deserialize_with`
  to EVERY `Vec<T>` field across Fingerprint (root_commits/all_commits/branches),
  Group.copies, QuarantineItem.unreachable_commits, UnionRemote.branches, Decision
  (quarantine/redundant/union_remotes), ApplyResult (skipped_files/actions), Plan
  (roots/decisions/third_party). Empty vec → `null`; non-empty → array; `null` →
  empty vec on load. Reproduces Go's nil-slice-vs-array JSON distinction.
- **MODEL CHANGE (parity fix 2) — zero-time `0001-01-01T00:00:00Z`.** Added `go_time`
  serde helper in `src/model.rs` on Fingerprint.last_commit + dir_mtime. Emits
  `%Y-%m-%dT%H:%M:%SZ` when nanos==0 (whole seconds, trailing `Z`, no fractional),
  else RFC3339 with `Z` + trimmed fractional groups. Replaces chrono's default
  `+00:00`. Deserialize parses both `Z` and offset forms → `DateTime<Utc>`.
- JSON shape: `serde_json::to_string_pretty` (2-space indent, declaration-order
  keys) written with NO trailing newline (matches Go MarshalIndent). Field order in
  model structs already matches Go — unchanged.
- divergence.md / MANIFEST.md built by string concat mirroring the Go `fmt.Fprintf`
  sequence exactly; `go_slice_v` reproduces Go `%v` on `[]string` → `[a b c]`.
- Golden fixtures copied verbatim to `tests/golden/{plan.json,divergence.md,MANIFEST.md}`.
  Golden test reads them via `CARGO_MANIFEST_DIR`.
- BYTE-PARITY CONCERNS for conductor to verify (this module WILL likely need
  byte-diff iteration):
  1. chrono `%Y` must zero-pad year 1 to `0001` (believed correct; VERIFY).
  2. `serde_json::to_string_pretty` array/null/number/bool formatting must match Go
     `json.MarshalIndent` char-for-char (2-space indent confirmed; verify nested
     empty containers — none present in golden).
  3. Go escapes `<>&` to `\u00XX`; serde_json does NOT. Golden has none of these
     chars, so no divergence here — but any future data with them would differ.
  4. Trailing-newline state: plan.json NONE, divergence.md ends `\n\n`,
     MANIFEST.md ends `\n`. Written exactly; verify golden files preserved LF (no
     CRLF/autocrlf mangling on Windows checkout — check `.gitattributes`).
  5. The em-dash `—` (U+2014) in divergence.md/MANIFEST.md headers is UTF-8 in both
     source and port; confirm no encoding drift.
  6. csv terminator forced to LF; Go csv default LF. Tests only assert Contains/
     HasPrefix so terminator is loose there.
- NOT exec-verified: porter had no Bash/exec. Conductor must run `cargo test`
  (fail→green: `report` was gated behind `// pub mod report;`), `cargo build`,
  iterate on any golden byte-diff, fill provenance sha256 (`PENDING-EXEC`), set
  row 6 verified, commit.

## safety (module 7) — high FS complexity
- Dependencies added: **none** (reuses `sha2`, `hex`, `walkdir`; std `fs`/`io`;
  consumes `crate::model::{Plan, StrategyKind, ...}` and `crate::gitx::Runner`).
- Public API:
  - `struct Violation { pub repo: String, pub machine: String, pub sha: String }`
    (Go Repo/Machine/SHA → snake_case per glossary).
  - `reachability_proof(p: &Plan) -> Vec<Violation>` (Go `ReachabilityProof`).
  - `physical_reachability(r: &dyn Runner, p: &Plan) -> Vec<Violation>` — Go
    `PhysicalReachability`, `context.Context` DROPPED. Git spec `<sha>^{commit}`;
    Fake match key `cat-file -e <sha>^{commit}`.
  - `copy_tree(src, dst, skip: &[String], dry_run) -> io::Result<Vec<String>>`.
  - `copy_tree_atomic(src, dst, skip, dry_run) -> io::Result<Vec<String>>`.
  - `tree_hash(root, skip: &[String]) -> io::Result<String>`.
  - private helpers: `file_sha256`, `copy_file`, `remove_all`, `file_mode`, `to_slash`.
- FAITHFUL walk: `filepath.Walk` → `walkdir::WalkDir::into_iter()` with
  `skip_current_dir()` == `filepath.SkipDir`. Per-entry walk errors append the
  entry path to `skipped` and continue (do NOT abort) — incl. the nonexistent-src
  case: walkdir yields one Err whose `.path()==src`, so `copy_tree(missing)` →
  `Ok(vec![src])` (matching Go's info==nil callback), and the atomic error surfaces
  later from the dst/rename steps. `rel=="."` reproduced via `entry.depth()==0`.
- `remove_all` mirrors Go `os.RemoveAll`: NotFound → Ok(()); else remove file/dir.
- TreeHash framing: `write!` into a `Sha256` (Digest impls io::Write) the exact
  bytes `path:{rel}\x00mode:{mode:o}\x00sha256:{hex}\x00`, entries sorted by rel
  (byte order). Always skips `.git` + `skip` dirs.
- MODE PARITY scheme (documented): `#[cfg(unix)]` uses
  `permissions().mode()` (full st_mode-ish, like Go incl. bits);
  `#[cfg(windows)]` synthesizes 0o444 (readonly) / 0o666 — deterministic per file
  so identical trees hash equal. Absolute TreeHash value is platform-dependent and
  NOT cross-checked vs Go (already true in Go itself).
- Test 7 (rollback) is WINDOWS-ONLY (`if !cfg!(windows) { return; }`, mirrors Go's
  `runtime.GOOS` skip). Forces `remove_dir_all(dst)` to fail by holding a handle on
  `dst/locked.txt` opened via `std::os::windows::fs::OpenOptionsExt::share_mode(3)`
  = FILE_SHARE_READ|FILE_SHARE_WRITE, NO FILE_SHARE_DELETE. Handle kept alive
  (dropped after assertions) so the sharing violation persists through the call.
- PARITY concerns:
  - Windows `std::fs::remove_dir_all` vs Go `os.RemoveAll`: test 7 depends on it
    FAILING when a child is held open without delete-sharing. If Rust's impl
    succeeds where Go fails, test 7's rollback assertions won't be exercised
    (test-driven divergence to verify on the conductor's Windows run).
  - mode octal bytes are NOT cross-language identical (Go FileMode vs Rust
    st_mode); only same-platform determinism is required and satisfied.
  - walkdir skip_current_dir vs filepath.SkipDir: matches for tested cases.
- NOT exec-verified: porter had no Bash/exec. Conductor must run `cargo test`
  (fail→green: module was gated behind `// pub mod safety;`), `cargo build`,
  verify Windows test 7 on Windows, fill provenance sha256 (`PENDING-EXEC`), set
  row 7 verified, commit.

## consolidate (module 9) — orchestration core
- Dependencies added: **none** (std only: `std::collections::HashSet`, `std::fmt`).
  Consumes `crate::gitx::{Runner, GitError, Fake}`, `crate::model::{Plan, ApplyResult,
  ...}`, `crate::safety::{copy_tree_atomic, tree_hash}`.
- Go `log/slog` info logging DROPPED entirely (side-effect only, untested; adding a
  logging crate would be an unjustified dep). Go `context.Context` DROPPED (glossary
  contract).
- Public API:
  - `pub static DEFAULT_EXCLUDES: &[&str]` (order preserved from Go DefaultExcludes)
    + `pub fn default_excludes() -> Vec<String>`.
  - `pub struct Options { dest: String, dry_run: bool, include_generated: bool,
    exclude_dirs: Option<Vec<String>> }` derives `Default`. `exclude_dirs` is
    `Option` to model Go's nil-vs-non-nil slice: `excludes()` → `vec![]` if
    include_generated; `Some(v)` → `v.clone()` (even Some(empty) = Go non-nil empty);
    `None` → DEFAULT_EXCLUDES.
  - `pub enum Error { Io(std::io::Error), Git(GitError) }` (`From` both; `Display`/
    `source` chain). Chosen over `Box<dyn Error>` for a typed, faithful wrap.
  - `pub fn apply(r: &dyn Runner, p: &Plan, opts: &Options) -> Result<ApplyResult, Error>`
    (Go `Apply`, ctx dropped).
  - Private: `canonical_excludes` (appends `_quarantine`), `already_consolidated`,
    `wrap`/`wrap_io` (Go `fmt.Errorf("<prefix>: %w")` message reproduction).
- FAITHFUL details: dest-collision disambiguation is pure string append
  `format!("{dest}-{suffix}")` on the OS-separator path string (suffix = first-7 of
  root_commits[0] when len>=7 else machine); `seen` HashSet guard; union-remote dedup
  loop `while used.contains(&name) { name = format!("{}-{}", u.name, i); i+=1 }`;
  `remote remove` + per-branch `branch --force` ignore errors (idempotent); `remote
  add`/`fetch` propagate wrapped errors. canonical hash excludes `_quarantine` so a
  nested quarantine copy doesn't defeat idempotency.
- PARITY concerns (for conductor to verify):
  1. Error-message wrapping: `wrap`/`wrap_io` prepend `<prefix>: ` to the inner
     cause. For GitError this prepends to `.cause` (keeps args/dir/stderr) so
     `Display` reads `git <args> (in <dir>): <prefix>: <cause>: <stderr>`. Go wraps
     the whole error: `<prefix>: git <args>...`. Only tested via `is_err()` (test 6),
     so no assertion depends on the exact string — but note the ordering divergence.
  2. Path-string disambiguation is byte-identical to Go (append on the same path
     string built with the OS separator).
  3. Union dedup loop replicated exactly (test 1 asserts unioned==2 for duplicate
     names; not-exercised beyond the first rename since dry-run).
  4. Test 11 (`test_apply_is_idempotent_on_second_run`) shells out to real `git` —
     always runs (Go gates it behind `-short`); conductor MUST have `git` on PATH.
- NOT exec-verified: porter had no Bash/exec. Conductor must run `cargo test`
  (fail→green: module was gated behind `// pub mod consolidate;`), `cargo build`,
  ensure `git` on PATH for test 11, fill provenance sha256 (`PENDING-EXEC`), set
  row 9 verified, commit.

## app (module 10) — mantle config shim
- Dependencies added: **none**. `src/app.rs` enabled via `pub mod app;` in lib.rs.
- Public API: `struct Features{logging, observability, daemon: bool}`,
  `struct LoggerConfig{level, format, redact}`, `struct ObservabilityConfig{
  protocol, sample: f64, interval_secs: u64, runtime_metrics}`,
  `struct Base{environment, features, logger, observability}`,
  `struct App{base: Base}`, `fn default_base() -> Base`, `fn new() -> App`.
- **MANTLE BOUNDARY (faithful-scope, NOT a defect).** Go `app.App` squash-embeds
  `mantle/bootstrap.Base`, and `main.go` calls `bootstrap.Configure(...)`, which
  wires an ENTIRE framework runtime in cobra's PersistentPreRunE: viper config-file
  loading, otel observability pipeline, structured/redacting logger, daemon
  supervisor. reposmerge's own commands NEVER read that Runtime — it is inert
  framework plumbing. Per the porting rule "map an external framework, don't
  reimplement it", mantle's viper/otel/logger/daemon runtime is **out of scope and
  NOT reimplemented**. `app.rs` reproduces ONLY the inert CONFIG DATA of
  `DefaultBase()` (environment "dev", features.logging=true/observability=daemon=
  false, logger info/json/redact, observability grpc/1.0/15s/runtime-metrics). The
  exact mantle default values live in an external module (unreadable here); the
  seeded values are modeled per the documented defaults and simplified to the
  fields that matter — documented in the module doc comment. The struct is inert
  (never read by any command); a doc comment says so.
- Characterization test `new_seeds_default_base` written (source had no app test).

## cmd / main (module 11) — clap CLI
- Dependency added: **`clap` (features=["derive"])** — Go cobra → Rust clap; std
  has no arg parser. Alt: hand-roll — rejected (subcommands/defaults/help parity).
- `src/main.rs` REPLACES the placeholder. Derive `Cli` with global `--config/-c`
  (accepted, unused, parity) + `#[command(version)]` (= CARGO_PKG_VERSION), and 4
  subcommands `scan|plan|apply|verify` with EXACT Go flag names/defaults/help.
  Dispatch → `Result<(), Box<dyn Error>>`; on Err print just the error to stderr +
  `exit(1)` (cobra SilenceUsage=true parity — no usage dump).
- **Mantle global-flag decision:** accepted ONLY `--config/-c` (unused) for parity.
  The other mantle globals (`--env`, `--log-level`, `--verbose/-v`, `--quiet/-q`,
  `--log-format`, `--log-source`, `--no-redact`, `--otel`, `--otel-endpoint`,
  `--otel-protocol`, `--daemon`) are **intentionally OMITTED** as out-of-scope
  framework plumbing (they only feed the un-ported mantle runtime). Documented in
  main.rs module doc.
- **Scan concurrency:** bounded worker pool via `std::thread::scope` (NO new dep —
  no rayon). The in-scope slice is split with `chunks_mut(ceil(n/workers))` into
  ≤`workers` disjoint contiguous chunks, one thread per chunk; each index is
  fingerprinted by exactly one thread (safe mutation), replacing Go's semaphore +
  WaitGroup over goroutines mutating `inScope[i]`. `ExecRunner` (unit struct, Sync)
  shared by reference. `dir_mtime` from `fs::metadata(path).modified()` →
  `DateTime::<Utc>::from(SystemTime)`; errors leave zero-time.
- `default_workers()` = `available_parallelism()*2` capped 16 min 1 (Err → 1).
- `generated_at` = `Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)` (Go
  `time.Now().UTC().Format(time.RFC3339)` — seconds precision, `Z`).
- Faithful command bodies: scan (discover→pooled fingerprint→group→plan→
  write_inventory/write_plan + summary line), plan (load_plan→decide per group→
  write_plan), apply (reachability_proof gate→consolidate::apply→optional
  physical_reachability→manifest→checksums, exact stderr/stdout wording + error
  strings), verify (reachability_proof + optional physical → OK / FAILED).
- Test `subcommands_registered` (port of Go `TestSubcommandsRegistered`) asserts
  the clap Command exposes scan/plan/apply/verify via `get_subcommands()`.

## e2e (integration test) — tests/e2e.rs
- Port of `internal/e2e/e2e_test.go::TestConsolidatePreservesAllCommits` as a Rust
  integration test over the public `reposmerge::` API. Builds two shared-lineage
  git repos (base + filesystem copy via `safety::copy_tree`), diverges each with a
  unique commit, runs discover→fingerprint→group→strategy→reachability→apply, then
  asserts `git -C <dest> log --all --format=%s` contains root/live-only/acer-only,
  `physical_reachability` empty, both sources' `.git` intact, and
  `decisions[0].strategy == StrategyKind::B`. Go's `-short` skip is DROPPED (Go
  skipped it under -short; the conductor always runs it with git available). Uses
  `std::process::Command` for git and OS-temp dirs (parity with `t.TempDir()`).

## PARITY concerns (modules 10/11/e2e)
- **Mantle framework boundary** (above) — the biggest scope decision; inert runtime
  not reimplemented; only CLI surface + inert config data reproduced.
- **`generated_at` non-determinism** — `Utc::now()` at scan time is inherently
  non-deterministic (as in Go); not byte-checkable, only shape-checked.
- **Worker-pool ordering** — fingerprints run concurrently; each index written by
  exactly one thread so results are order-independent (`group::build` re-derives
  deterministic order). No data race; no observable divergence vs Go's goroutine
  pool.
- **exec** — porter had no Bash/exec: could NOT run `cargo test`/`cargo build` or
  compute provenance sha256. Conductor MUST run `cargo build`, `cargo test`
  (fail→green; needs real `git` on PATH for the e2e + any real-git tests), fill
  provenance `PENDING-EXEC` hashes, flip rows 10/11/e2e `verified`, and commit.

## Deviations / gaps
- `app` (mantle shim): no source tests — characterization test written (see above).
- `model`: PARITY-VERIFY zero `time.Time`. Go zero value → JSON
  `"0001-01-01T00:00:00Z"` and `Unix() = -62135596800` → `/86400 = -719162`
  (trunc-toward-zero). Reproduced via `zero_time()` (year-1) + i64 truncating
  division. In `test_score_orders_by_ahead` the recency term is equal on both
  sides (both zero-time) so it cancels — no test divergence. Confirm RFC3339Nano
  string byte-equality against a Go golden once `report` lands.
- `model`: NOT exec-verified. Porter subagent had no Bash/exec tool → could not
  run `cargo test` (fail→green) or `cargo build`, nor compute provenance sha256
  or commit. Conductor must run the fail/green cycle, fill provenance hashes,
  and commit. Faithful port written; `verified` left ☐.
