# AGENTS.md
<!-- rev:001 (RFC 3339) 2026-07-22T19:46:58Z -->

Canonical cross-tool agent instructions for the `reposmerge` crate.

## Overview

`reposmerge` is a CLI that consolidates scattered git repo copies into one canonical tree without losing commits. It is a **faithful 1:1 Rust port** of the Go tool `github.com/inovacc/reposmerge`. The port is complete and byte-parity audited. Library modules live in `src/`; the binary is `src/main.rs`.

## Build / Test / Lint / Coverage / Format

```bash
cargo build                        # build lib + binary
cargo test                         # 52 tests (50 lib unit, 1 CLI, 1 e2e)
cargo llvm-cov --summary-only      # coverage (~86.6% line / 86.9% region / 81.6% function)
cargo clippy --all-targets         # lint
cargo fmt                          # format
```

Run the binary with `cargo run -- <subcommand> ...` (never `build && ./binary`). The e2e test and any real-repo operation require `git` on `PATH`.

## Code style

- Idiomatic Rust, **std-first** — a new dependency is added only when std cannot do the job, and must be logged in `PORT-TRACK.md` with a justification.
- **Faithful-port constraint (hard rule):** this crate mirrors the Go source 1:1. Any change to behavior, types, JSON shape, or output must preserve Go parity. Before changing a module, consult `PORT-GLOSSARY.md` (shared type/naming/error decisions) and `PORT-TRACK.md` (the per-module parity ledger + documented deviations). Report artifacts are byte-exact against Go goldens — do not alter serialization without re-verifying the golden.
- Naming: Go `CamelCase` types → Rust `CamelCase`; methods → `snake_case`; serde `rename` preserves exact Go JSON keys (e.g. `FP`, `RemoteURL`).
- Errors: `Result<T, E>` with per-module error enums; propagate with `?`.

## Testing conventions

- Unit tests live in-module under `#[cfg(test)] mod tests`.
- Golden fixtures live in `tests/golden/` (byte-exact `plan.json`, `divergence.md`, `MANIFEST.md`); the golden test loads them via `CARGO_MANIFEST_DIR`. Keep them LF-only (enforced by `.gitattributes`).
- The integration test `tests/e2e.rs` and real-git unit tests need `git` on `PATH`.
- One test (safety atomic-rollback) is Windows-only and exercises a sharing-violation rollback path.

## Security

- No secrets in the repo; never commit `.env`. The tool runs `git` locally and reads/writes the filesystem only.
- Git is local-only here (no remote configured).
- License: BSD-3-Clause.

## Commit conventions

- Conventional commits (`feat:`, `fix:`, `docs:`, `test:`, `chore:`).
- **No AI attribution** — no `Co-Authored-By: Claude` or similar trailers.
- Use the configured git `user.name` / `user.email`.
