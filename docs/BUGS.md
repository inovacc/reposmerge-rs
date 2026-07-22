# Bugs

_Generated: 2026-07-22_

## No known bugs

As of v1.0.0 there are **no known bugs**.

- All **55 tests pass** (50 lib incl. report byte-golden + Windows-gated safety rollback, 4 CLI, 1 real-git e2e).
- **Byte-parity vs the Go original is confirmed** by the differential Go-oracle audit — all 4 report artifacts (`plan.json`, `inventory.csv`, `third-party.csv`, `divergence.md`) are byte-identical.
- No `TODO`/`FIXME`/`XXX` markers exist in `src/`.
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --all --check` are clean; CI is green on linux/macos/windows.

Documented **intentional limitations** (not bugs) are tracked in [`ISSUES.md`](ISSUES.md): the mantle framework boundary, platform-dependent TreeHash, the `git`-on-`PATH` requirement, and the Windows-only rollback test.

Report a new bug at <https://github.com/inovacc/reposmerge-rs/issues>.
