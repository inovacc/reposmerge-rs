# Implementation Tasks

_Generated: 2026-07-22_

The faithful port is **done** (v1.0.0 released, byte-parity audited, CI + release live). Only the tasks below remain. See [`BACKLOG.md`](BACKLOG.md) for full context and priorities.

| ID | What | Files | Effort |
|----|------|-------|--------|
| T-01 | Optional CI action-version bumps as they age (e.g. keep `actions/*` current). | `.github/workflows/ci.yml`, `.github/workflows/release.yml` | S |
| T-02 | Cross-platform TreeHash normalization — normalize file-mode bits so tree hashes match across OSes. **Only if** cross-platform hash equality becomes a requirement. | `src/safety.rs` | M |

Effort key: S ≈ <1 day, M ≈ 1–3 days, L ≈ >3 days.

Cross-reference: T-02 → BACKLOG P3 "Cross-platform TreeHash normalization".

**Not planned:** crates.io publish — this crate ships via GitHub Releases only (`publish = false`).
