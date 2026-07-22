# Milestones

_Generated: 2026-07-22_

## v1.0.0 — Faithful port complete (RELEASED 2026-07-22)

- Faithful 1:1 Rust port of Go `github.com/inovacc/reposmerge` complete — all 11 port-units (model, gitx, fingerprint, group, discover, report, safety, strategy, consolidate, app, cmd/main) ported in dependency order.
- **Byte-parity audited** — all 4 report artifacts (`plan.json`, `inventory.csv`, `third-party.csv`, `divergence.md`) byte-identical to the Go oracle; provenance signed to Go commit `479a7c58`.
- **55 tests** — 50 lib unit tests (incl. report byte-golden + Windows-gated safety rollback), 4 CLI tests, 1 real-git e2e test. All passing.
- **Coverage:** 88.20% line · 87.58% region · 83.18% function (cargo-llvm-cov).
- **CI + release live** — CI green on ubuntu/macos/windows + coverage job; `release.yml` publishes 5 cross-platform binaries on a `v*` tag.
- **Git tag `v1.0.0`** released at <https://github.com/inovacc/reposmerge-rs/releases/tag/v1.0.0>.

## v1.1.0 — Post-release polish (tentative, next)

- Publish to **crates.io** as `reposmerge` (remote dependency now satisfied; no known name collision).
- Any post-release polish surfaced after publish (optional CI action-version bumps, optional cross-platform TreeHash normalization if needed).
