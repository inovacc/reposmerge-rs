# PORT-TRACK — reposmerge Go → Rust

Source: `github.com/inovacc/reposmerge` @ `479a7c585fad56d4330a1136e5f19b682d02c609`
Pair: go2rust · Scope: full 1:1 parity · Target: `../reposmerge-rs`

## Modules (dependency order) — 11 port-units

| # | module | tests-ported | code-ported | verified | parity | commit |
|---|--------|:---:|:---:|:---:|--------|--------|
| 1 | model       | ☑ | ☑ | ☑ | PASS (1 test) | (see git) |
| 2 | gitx        | ☐ | ☐ | ☐ | — | — |
| 3 | fingerprint | ☐ | ☐ | ☐ | — | — |
| 4 | group       | ☐ | ☐ | ☐ | — | — |
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
