# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- **Smart-match grouping (deliberate deviation from the Go oracle, default-on).**
  Repo identity is now keyed **lineage-first** (the root-commit set) instead of
  remote-URL-first. This unifies copies the Go original wrongly split into
  separate groups: (1) a remote-backed copy and a local-only copy of the *same*
  repo (same root commit, remote stripped/changed from one); (2) case-variant
  remote URLs (`inovacc/CaseTest` vs `inovacc/casetest`). It is a strict superset
  of Go's behavior — same-lineage copies Go already merged still merge, genuinely
  divergent lineages still stay separate — and remains fully lossless (`verify`
  still proves every source commit survives). Grouping output therefore
  intentionally diverges from the Go tool; report *serialization* parity is
  unaffected. See `src/group.rs`, `docs/ISSUES.md`, and `PORT-TRACK.md`.

### Added
- Dependabot for GitHub Actions + Cargo updates.
- CI supply-chain gate via `cargo-deny` (advisories, bans, licenses, sources) + `deny.toml`.
- README status badges (CI, release, license).
- `SECURITY.md` and issue/PR templates.
- Smart-match unit tests (remote+local unify, case-variant unify, divergent-lineage stays split) and error-wrap coverage tests.
- Full-pipeline physical-verify CLI gate (`tests/cli.rs`) proving the binary consolidates and self-proves no loss.

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
