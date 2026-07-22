# Contributors

<!-- rev:001 (RFC 3339) 2026-07-22T23:47:23Z -->

## Maintainer

- **dyammarcano** &lt;dyam.marcano@gmail.com&gt; (inovacc org) — owner / maintainer.

Repository: <https://github.com/inovacc/reposmerge-rs>

## How to contribute

1. Fork and branch from `main` (`feat/…`, `fix/…`, `docs/…`).
2. Make your change with a test that covers it (test-first is preferred — see the port ledgers below).
3. Run the full local gate (below) and ensure it is green.
4. Open a PR; all four CI jobs (linux/macos/windows build+test and the coverage job) must pass.

## Development commands

| Task | Command |
|------|---------|
| Build | `cargo build` |
| Test | `cargo test` |
| Coverage | `cargo llvm-cov --summary-only` |
| Lint | `cargo clippy --all-targets -- -D warnings` |
| Format check | `cargo fmt --all --check` |

`cargo clippy` must be **warning-free** (`-D warnings` is a hard gate) and `cargo fmt --all --check` must report no diffs.

## MSRV

Minimum Supported Rust Version is pinned to **1.74** (see `clippy.toml`). Do not use language or stdlib features newer than 1.74.

## Commit convention

- **Conventional Commits** (`feat:`, `fix:`, `docs:`, `test:`, `chore:`, `ci:`, `build:`, `style:`, `refactor:`).
- **No AI attribution** — do not add `Co-Authored-By: Claude` or any AI trailer.
- Use your configured git `user.name` / `user.email`.

## Faithful-port constraint (MANDATORY)

`reposmerge` is a **faithful 1:1 Rust port** of Go `github.com/inovacc/reposmerge`, byte-parity audited and signed to Go source commit `479a7c58` in `PORT-PROVENANCE.json`. Any change that touches observable behavior — report artifacts (`plan.json`, `inventory.csv`, `third-party.csv`, `divergence.md`), CLI flags/defaults, strategy decisions — **must preserve Go byte-parity**. Before changing such code:

- Read the parity ledger [`PORT-TRACK.md`](../PORT-TRACK.md) (per-module/test/dependency/deviation record).
- Read [`PORT-GLOSSARY.md`](../PORT-GLOSSARY.md) (shared type/naming/error decisions — serde PascalCase, null-slice, `go_time` helpers, `path.Clean` port).
- Keep the golden fixtures in `tests/golden/` byte-identical, or justify the change against the Go oracle.

Behavior divergences must be documented as intentional deviations in `PORT-TRACK.md`, not slipped in silently.
