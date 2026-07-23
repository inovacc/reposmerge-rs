# Backlog

_Generated: 2026-07-22_

Prioritized future work and tech debt. The port itself is complete and byte-parity audited; these are post-port items. No `TODO`/`FIXME`/`XXX` markers currently exist in `src/`.

## P1 — release readiness

_All P1 items are complete — see Done below. The crate ships via GitHub Releases only (crates.io publish decided against — see "Won't do")._

### Done (2026-07-22)
- ~~Add a git remote + publish~~ — remote live at <https://github.com/inovacc/reposmerge-rs>; pushed and tag `v1.0.0` released with 5 cross-platform binaries (commits `9c4652d`/`bebb878`).
- ~~CI workflow~~ — added `.github/workflows/ci.yml`: fmt-check + `clippy -D warnings` + build + test (linux/macos/windows) + `cargo-llvm-cov` (commit `1c5701b`).
- ~~Release packaging~~ — added `.github/workflows/release.yml`: 5-target cross-platform binaries on a `v*` tag (commit `1c5701b`).
- ~~Cargo publish metadata~~ — authors/repository/keywords/categories/readme/rust-version (commit `da3463f`).
- ~~Clippy-clean gate + fmt/clippy config~~ — `rustfmt.toml`/`clippy.toml`; all lints resolved (commit `7e14d42`).
- ~~CLI branch coverage~~ — tests for `run_verify` (ok/loss) and `run_apply` (dry-run) (commit `7e14d42`).

## P2 — parity deviations to track (from PORT-TRACK.md)

These are documented, intentional deviations — track them in case downstream needs change.

| Item | Notes | Effort |
|------|-------|--------|
| Mantle framework boundary | The mantle runtime (viper config, otel, logger, daemon) is NOT reimplemented; `app.rs` reproduces only inert config data. Revisit only if the observability/logging global flags are ever required. | L |
| `consolidate::Error` wrap-order | Cosmetic: Rust prepends the error prefix to the inner cause, so `Display` ordering differs from Go's whole-error wrap. Only tested via `is_err()`; no assertion depends on the string. | S |
| TreeHash platform mode bits | The absolute TreeHash value is platform-dependent (Windows synthesizes 0o444/0o666 vs Unix `st_mode`). Only same-platform determinism is guaranteed — matches Go's own behavior. | M |

## P3 — enhancements

| Item | Notes | Effort |
|------|-------|--------|
| Cross-platform TreeHash normalization | Only if cross-platform hash equality becomes a requirement. | M |
| CI action-version bumps | Keep `actions/*` current as they age (e.g. `actions/checkout` bumped v4→v5 in `bebb878`). Low priority, ongoing. | S |

## Won't do

| Item | Decision |
|------|----------|
| Publish to crates.io | **Decided against** — this crate is distributed via GitHub Releases only, not crates.io. Enforced with `publish = false` in `Cargo.toml`. |

Effort key: S ≈ <1 day, M ≈ 1–3 days, L ≈ >3 days.
