# Backlog

_Generated: 2026-07-22_

Prioritized future work and tech debt. The port itself is complete and byte-parity audited; these are post-port items. No `TODO`/`FIXME`/`XXX` markers currently exist in `src/`.

## P1 — release readiness

| Item | Notes | Effort |
|------|-------|--------|
| Add a git remote + publish | Repository is currently **local-only** (no remote configured). Create a remote and push. **Needs a remote URL — blocked on input.** | S |

### Done (2026-07-22)
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
| Publish to crates.io | **Decision:** publish as `reposmerge` — the Go original is not on crates.io, so no name collision is expected (fall back to a suffixed name if taken). Blocked on adding a git remote first. | S |
| Cross-platform TreeHash normalization | Only if cross-platform hash equality becomes a requirement. | M |

Effort key: S ≈ <1 day, M ≈ 1–3 days, L ≈ >3 days.
