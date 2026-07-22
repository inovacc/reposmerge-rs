# PORT-TRACK — reposmerge Go → Rust

Source: `github.com/inovacc/reposmerge` @ `479a7c585fad56d4330a1136e5f19b682d02c609`
Pair: go2rust · Scope: full 1:1 parity · Target: `../reposmerge-rs`

## Modules (dependency order) — 11 port-units

| # | module | tests-ported | code-ported | verified | parity | commit |
|---|--------|:---:|:---:|:---:|--------|--------|
| 1 | model       | ☑ | ☑ | ☑ | PASS (1 test) | (see git) |
| 2 | gitx        | ☑ | ☑ | ☑ | PASS (2 tests total) | (see git) |
| 3 | fingerprint | ☑ | ☑ | ☑ | PASS (3 tests total) | (see git) |
| 4 | group       | ☑ | ☑ | ☑ | PASS (6 tests total) | (see git) |
| 5 | discover    | ☐ | ☐ | ☐ | — | — |
| 6 | report      | ☐ | ☐ | ☐ | — | — |
| 7 | safety      | ☐ | ☐ | ☐ | — | — |
| 8 | strategy    | ☐ | ☐ | ☐ | — | — |
| 9 | consolidate | ☐ | ☐ | ☐ | — | — |
| 10| app         | ☐ | ☐ | ☐ | — | — |
| 11| cmd (main)  | ☐ | ☐ | ☐ | — | — |
|  + | e2e (tests/) | ☐ | ☐ | ☐ | — | — |

## Dependencies added
- `serde` (+ derive) — JSON (de)serialization parity for `report` module; std has
  no serialization. Alt considered: hand-rolled — rejected (reinvention defect).
- `chrono` (features=["serde"]) — RFC3339/calendar time parity for `LastCommit`/
  `DirMtime`; `std::time` cannot format calendar dates or the Go zero-time
  `0001-01-01T00:00:00Z`. Alt: `time` crate — chrono chosen for serde+RFC3339Nano.
- `serde_json` — deferred to `report` module (not needed by `model` itself).

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

## Deviations / gaps
- `app` (mantle shim): no source tests — write characterization test before porting.
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
