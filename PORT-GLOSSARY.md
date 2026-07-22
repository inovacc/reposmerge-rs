# PORT-GLOSSARY — shared type/naming/error decisions (Go → Rust)

Every module porter READS this before porting and APPENDS any new shared decision.
Keeps modules ported in independent contexts coherent (cross-module type identity).

## Naming
- Go exported `CamelCase` types → Rust `CamelCase`; funcs/methods → `snake_case`.
- Go package-qualified `model.Copy` → Rust `crate::model::Copy`.
- Keep field names semantically identical; Rust struct fields `snake_case`
  (serde `rename`/`rename_all` used only where JSON key parity requires it).

## Error style
- Go `(T, error)` → `Result<T, ...>`. Per-module error enum; propagate with `?`.
- Until a module needs richer errors, `Result<T, std::io::Error>` or a module
  `Error` enum. Revisit if a `thiserror`-style aggregate is justified (log it).

## Core shared types (populated by `model` port)
All in `crate::model`. Structs derive `Debug, Clone, Default, PartialEq,
Serialize, Deserialize`; every field has explicit `#[serde(rename = "...")]`
giving exact Go PascalCase JSON keys (acronyms `FP`, `RemoteURL`, `RepoName`
kept verbatim — do NOT use `rename_all`, it would mangle them).

| Go | Rust | notes |
|----|------|-------|
| `model.Copy` | `Copy` | fields: path, root, machine, owner, repo_name, remote_url, fp |
| `model.Branch` | `Branch` | name, tip |
| `model.Fingerprint` | `Fingerprint` | int counters → `i64` (Go platform `int` = 64-bit); `time.Time` → `DateTime<Utc>`; has `score() -> i64` |
| `model.StrategyKind` | `StrategyKind` | enum `A`/`B`/`C`, serde-renamed to `"A-richest-quarantine"`/`"B-union-branches"`/`"C-snapshot"`; `Default = A` |
| `model.Group` | `Group` | key, owner, repo_name, has_remote, remote_url, copies |
| `model.QuarantineItem` | `QuarantineItem` | copy, dest_path, reason, unreachable_commits |
| `model.UnionRemote` | `UnionRemote` | name, path, branches |
| `model.Decision` | `Decision` | group, strategy, canonical, canonical_reason, dest_path, quarantine, redundant, union_remotes |
| `model.ApplyResult` | `ApplyResult` | copied, quarantined, unioned, skipped (i64), skipped_files, actions |
| `model.Plan` | `Plan` | roots, dest, generated_at, decisions, third_party |

Zero `time.Time` reproduced via `zero_time()` = year-1 `0001-01-01T00:00:00Z`
in `Fingerprint::Default` (chrono supports year 1). PARITY-VERIFY at `report`.

## gitx `Runner` — cross-module identity (fingerprint, safety, consolidate depend on this)
- Go `Runner.Run(ctx, dir string, args ...string) (string, error)` →
  Rust `trait Runner { fn run(&self, dir: &str, args: &[&str]) -> Result<String, GitError>; }`
  in `crate::gitx`.
  - **`context.Context` is DROPPED** — no cancellation tested; omit it in every
    downstream caller too (do NOT reintroduce a ctx/cancel param).
  - `run` takes **`&self`** (not `&mut self`) so callers can hold `&dyn Runner`
    and call repeatedly. Any Runner needing to record state (like `Fake`) uses
    **interior mutability** (`RefCell`).
- Error type: **`gitx::GitError`** (pub struct: `args`, `dir`, `cause`, `stderr`)
  impl `Display` + `std::error::Error`. Display =
  `git <args joined " "> (in <dir>): <cause>: <trimmed stderr>` (faithful to Go).
  Downstream modules propagate it with `?` (may wrap in their own error enum).
- Constructors: `gitx::new_runner() -> ExecRunner` (Go `New`), also
  `ExecRunner::new()`. Test double: `gitx::Fake::new()` (Go `NewFake`) with
  builders `.with_response(key, val)` / `.with_error(key, val)`; pub fields
  `responses`, `errs` (`HashMap<String,String>`) and `calls: RefCell<Vec<String>>`
  (`.calls()` returns a snapshot). Match key = `args.join(" ")`.
- `gitx::is_repo(dir: &Path) -> bool` (Go `IsRepo`) — checks `dir/.git` exists.

## discover module — shared types/decisions (module 5)
- `discover::Scope { in_scope_owners: Vec<String>, third_party_dirs: Vec<String> }`
  (pub fields); `discover::default_scope() -> Scope` (Go `DefaultScope`).
- Free fns (snake_case): `normalize_url`, `parse_owner_repo`, `infer_machine`,
  `source_disc`, `discover`, `pub(crate) is_third_party`. `discover(...)` returns
  `Result<(Vec<Copy>, Vec<Copy>), gitx::GitError>` = (in_scope, third_party); Go
  `context.Context` DROPPED.
- Deps introduced (reuse, don't re-add): `sha2` (Sha256), `hex`, `walkdir`.
- `source_disc` token = first 6 hex chars of `sha256(ToSlash(Dir(path)))`; parent-dir
  extraction normalizes to '/', strips one trailing '/', cuts at last '/'.

## JSON / serialization parity (report module) — CRITICAL
- Model structs carry **no `json:` tags**, so Go marshals exported fields with
  their **exact PascalCase** names: `Path`, `Root`, `Machine`, `Owner`,
  `RepoName`, `RemoteURL`, `FP`, `Head`, `Ahead`, `Behind`, etc. The port MUST
  reproduce PascalCase JSON keys → put `#[serde(rename = "...")]` on each field
  (or `#[serde(rename_all = "PascalCase")]` on the struct — but verify acronym
  cases like `FP`, `RemoteURL`, `RepoName` which `rename_all` may mangle; prefer
  explicit per-field `rename` for those).
- `json.MarshalIndent(v, "", "  ")` → 2-space pretty; key order = struct field
  declaration order (serde preserves declaration order). LF newlines.
- Go `time.Time` marshals as **RFC3339Nano** (`chrono` DateTime must format
  identically); the zero `time.Time` marshals as `"0001-01-01T00:00:00Z"`.
- Go `nil` slice marshals as `null`, empty non-nil slice as `[]` — match the
  source's slice initialization to keep golden bytes identical.
