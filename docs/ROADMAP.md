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

- **55 tests total** — 50 lib unit tests (including a byte-exact golden in `report` and a Windows-gated atomic-rollback test in `safety`), 4 CLI tests (subcommand registration + 3 branch tests), 1 real-git e2e test.
- **Coverage (cargo-llvm-cov):** 88.20% line · 87.58% region · 83.18% function. Lowest module is `main.rs` at 52.7% (CLI wiring — exercised mainly via the real-binary differential rather than unit tests).
- Every module carries tests: model, gitx, fingerprint, group, discover, report (byte-golden), safety (Windows rollback), strategy, consolidate (real-git), main (CLI), e2e (full pipeline). See `PORT-TRACK.md` for the per-module ledger.

## Phase 1 — Release & distribution (complete)

- [x] CI workflow — fmt-check + `clippy -D warnings` + build + test on linux/macos/windows, plus a `cargo-llvm-cov` coverage job (`.github/workflows/ci.yml`, commit `1c5701b`).
- [x] Clippy-clean gate + `rustfmt.toml`/`clippy.toml` config (commit `7e14d42`).
- [x] Release packaging — cross-platform binaries for 5 targets on a `v*` tag (`.github/workflows/release.yml`, commit `1c5701b`). **SHIPPED** — tag `v1.0.0` released, workflow green, 5 binaries published.
- [x] Cargo publish metadata (authors/repository/keywords/categories/readme/rust-version, commit `da3463f`).
- [x] Add a git remote and push the repository — remote live at <https://github.com/inovacc/reposmerge-rs> (commits `9c4652d`/`bebb878`).
- [x] Distribution decided: **GitHub Releases only** — not published to crates.io (`publish = false` in `Cargo.toml`).

## Phase 2 — Post-parity enhancements (backlog)

- [ ] Revisit the mantle framework boundary if the observability/logging flags are ever needed (see `docs/ISSUES.md`).
- [ ] Cross-platform TreeHash normalization if cross-platform hash equality becomes a requirement.

See `docs/BACKLOG.md` for the detailed item list and `docs/ISSUES.md` for known limitations.
