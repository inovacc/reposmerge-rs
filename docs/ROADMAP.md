# reposmerge — Roadmap

_Generated: 2026-07-22_

## Phase 0 — Faithful Go → Rust port (COMPLETE)

The entire tool has been ported 1:1 from `github.com/inovacc/reposmerge` (Go source commit `479a7c58`) and audited to byte-parity.

- [x] All 11 port-units ported in dependency order: model, gitx, fingerprint, group, discover, report, safety, strategy, consolidate, app, cmd (main).
- [x] Differential Go-oracle audit — all 4 report artifacts (`plan.json`, `inventory.csv`, `third-party.csv`, `divergence.md`) BYTE-IDENTICAL to Go (3 findings fixed: DestPath clean, timestamp timezone, fractional-seconds trimming).
- [x] Real-git end-to-end test (`tests/e2e.rs`) verifying the full scan→plan→apply→verify pipeline preserves every commit.
- [x] CLI surface reproduced (clap): `scan`, `plan`, `apply`, `verify` with exact Go flag names/defaults; `--config/-c` + `--version` for parity.
- [x] Provenance signed in `PORT-PROVENANCE.json`; ledgers `PORT-TRACK.md` / `PORT-GLOSSARY.md` maintained.

## Test Coverage

- **52 tests total** — 50 lib unit tests (including a byte-exact golden in `report` and a Windows atomic-rollback test in `safety`), 1 CLI subcommand-registration test, 1 real-git e2e test.
- **Coverage (cargo-llvm-cov):** ~86.6% line · 86.9% region · 81.6% function.
- Every module carries tests: model (1), gitx (2), fingerprint (3), group (6 cumulative), discover (13 cumulative), report (18 cumulative, byte-golden), safety (32 cumulative, Windows rollback), strategy (38 cumulative), consolidate (49 cumulative, real-git), main (CLI subcommands), e2e (full pipeline). See `PORT-TRACK.md` for the per-module ledger.

## Phase 1 — Release & distribution (planned)

- [ ] Add a git remote and publish the repository (currently local-only).
- [ ] CI workflow (build + `cargo test` + `cargo clippy` + `cargo fmt --check` + coverage) on push/PR.
- [ ] Release packaging — cross-platform binaries via `cargo-dist` (GoReleaser-equivalent).
- [ ] Consider publishing to crates.io (or document why not, given the name collision with the Go tool).

## Phase 2 — Post-parity enhancements (backlog)

- [ ] Revisit the mantle framework boundary if the observability/logging flags are ever needed (see `docs/ISSUES.md`).
- [ ] Cross-platform TreeHash normalization if cross-platform hash equality becomes a requirement.

See `docs/BACKLOG.md` for the detailed item list and `docs/ISSUES.md` for known limitations.
