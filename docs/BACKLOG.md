# Backlog

_Generated: 2026-07-22_

Prioritized future work and tech debt. The port itself is complete and byte-parity audited; these are post-port items. No `TODO`/`FIXME`/`XXX` markers currently exist in `src/`.

## P1 — release readiness

| Item | Notes | Effort |
|------|-------|--------|
| Add a git remote + publish | Repository is currently **local-only** (no remote configured). Create a remote and push. | S |
| CI workflow | No CI exists. Add build + `cargo test` + `cargo clippy --all-targets` + `cargo fmt --check` + `cargo llvm-cov` on push/PR. | M |
| Release packaging | No release/distribution setup. Add `cargo-dist` (GoReleaser-equivalent) for cross-platform binaries. | M |

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
| Publish to crates.io | Or document why not (name collision with the Go tool). | S |
| Cross-platform TreeHash normalization | Only if cross-platform hash equality becomes a requirement. | M |

Effort key: S ≈ <1 day, M ≈ 1–3 days, L ≈ >3 days.
