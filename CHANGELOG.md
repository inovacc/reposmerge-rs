# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Dependabot for GitHub Actions + Cargo updates.
- CI supply-chain gate via `cargo-deny` (advisories, bans, licenses, sources) + `deny.toml`.
- README status badges (CI, release, license).
- `SECURITY.md` and issue/PR templates.

## [1.0.0] - 2026-07-22

Initial release: a faithful 1:1 Rust port of the Go tool
[github.com/inovacc/reposmerge](https://github.com/inovacc/reposmerge), byte-parity
audited against the Go source.

### Added
- `scan → plan → apply → verify` CLI (clap), read-only on sources until `--confirm`.
- A/B/C consolidation strategies (richest-wins+quarantine / union-branches / snapshot).
- Pre-apply and physical (post-apply) reachability proofs — no commit is ever lost.
- Resilient atomic `CopyTree` and idempotent `apply`.
- `TreeHash` working-tree integrity hashing.
- JSON/CSV/markdown reports with byte-exact output parity vs the Go tool.
- Nested-repo discovery and bounded-parallel fingerprinting.
- Cross-platform release binaries (linux x86_64/aarch64, macOS x86_64/aarch64, windows x86_64).
- Provenance signed to Go source commit `479a7c58`.

[Unreleased]: https://github.com/inovacc/reposmerge-rs/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/inovacc/reposmerge-rs/releases/tag/v1.0.0
