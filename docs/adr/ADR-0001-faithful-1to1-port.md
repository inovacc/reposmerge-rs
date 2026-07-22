# ADR-0001: Faithful 1:1 byte-parity port of Go reposmerge to Rust

- **Status:** Accepted
- **Date:** 2026-07-22
- **Deciders:** dyammarcano (inovacc)

## Context

`github.com/inovacc/reposmerge` is an existing, working Go CLI that consolidates duplicated/scattered git repo working-tree copies into one canonical tree while proving no commit is lost. We want a Rust implementation with **identical** observable behavior — same CLI surface, same strategy decisions, same on-disk report artifacts — rather than a reinterpretation. The Go tool also embeds the `mantle` framework (viper config, otel observability, structured logger, daemon supervisor) that its own commands never actually read.

## Decision

Port Go → Rust as a **faithful 1:1 byte-parity port, not a rewrite**. Concretely:

- **Test-first, module-by-module** in dependency order (model → gitx → fingerprint → group → discover → report → safety → strategy → consolidate → app → main): port each module's tests, then the code to pass them.
- **Standard-library-first dependencies** — pull a crate only when the port forces it, and log each in `PORT-TRACK.md` (serde/serde_json, chrono, csv, sha2/hex, walkdir, clap).
- **Provenance signing** — record the Go source commit (`479a7c58`) and per-file hashes in `PORT-PROVENANCE.json`; maintain `PORT-TRACK.md` (parity ledger) and `PORT-GLOSSARY.md` (type/naming/error decisions).
- **Parity helpers** to match Go serialization exactly: serde PascalCase field names, null-vs-empty slice handling, a `go_time` RFC3339/calendar helper, and a `path.Clean` port for path normalization.
- **Framework boundary** — the mantle runtime is mapped, not reimplemented: `app.rs` reproduces only the inert config data of `DefaultBase()`, and only the observable CLI surface (`--config/-c` no-op, `--version`, the four subcommands with exact flag names/defaults) is reproduced.

## Consequences

- **Byte-parity is verified** via a Go-oracle differential audit — all 4 report artifacts (`plan.json`, `inventory.csv`, `third-party.csv`, `divergence.md`) are byte-identical (3 findings fixed during the audit: DestPath clean, timestamp timezone, fractional-seconds trimming).
- The **mantle framework runtime is an out-of-scope boundary**, reproduced only as an inert config shim; the mantle global flags (`--env`, `--log-level`, `--otel*`, `--daemon`, …) are intentionally omitted. Revisit only if those flags are ever genuinely needed.
- Behavior changes are constrained: any future change touching reports, CLI flags, or strategy decisions must preserve Go byte-parity or be recorded as an intentional deviation in `PORT-TRACK.md`.
- **TreeHash is platform-dependent** (mode bits differ Unix vs Windows), matching Go's own behavior — only same-platform determinism is guaranteed.
