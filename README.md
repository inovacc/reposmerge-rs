# reposmerge
<!-- rev:003 (RFC 3339) 2026-07-23T00:07:00Z -->

[![CI](https://github.com/inovacc/reposmerge-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/inovacc/reposmerge-rs/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/inovacc/reposmerge-rs?sort=semver)](https://github.com/inovacc/reposmerge-rs/releases/latest)
[![License: BSD-3-Clause](https://img.shields.io/badge/license-BSD--3--Clause-blue.svg)](LICENSE)

> Consolidate duplicated/scattered git repository copies into one canonical tree — without losing a single commit.

`reposmerge` is a CLI that finds every scattered working-tree copy of your git repositories, groups the copies that belong together, decides how to merge each group safely, and consolidates them into a canonical tree while proving that no commit is ever lost. It is a **faithful 1:1 Rust port** of the Go tool [github.com/inovacc/reposmerge](https://github.com/inovacc/reposmerge) (byte-parity audited against the Go source — see [Provenance](#provenance)).

## Features

- **Four-step pipeline** — `scan → plan → apply → verify`, each stage read-only on your sources until you explicitly `--confirm`.
- **A/B/C consolidation strategies** decided per group:
  - **A — richest-wins + quarantine** for groups with a known remote upstream (keeps the richest canonical copy, quarantines divergent history).
  - **B — union-all-history** for purely local groups with shared lineage (unions every copy's branches into the canonical repo).
  - **C — verbatim snapshot** for name collisions that cannot be automatically reconciled.
- **Commit-reachability proofs** — a static plan proof runs before any write and aborts if any source commit would become unreachable; an optional physical proof queries the real consolidated repos after apply.
- **Atomic copy** — each repo is copied into a temporary directory then renamed into place, with rollback, so a failure never leaves a partial tree.
- **Idempotency** — a repo whose destination already matches by content hash is skipped, so re-runs are safe.
- **Integrity manifest** — a confirmed run writes `reports/checksums.sha256` over the consolidated tree.
- **Byte-parity port** — report artifacts (`plan.json`, `inventory.csv`, `third-party.csv`, `divergence.md`) are byte-identical to the Go original.

## Install / Build

Requires `git` on `PATH` for real-repo operations.

**Prebuilt binaries** — download a `v1.0.0` binary for your platform (5 targets) from the [GitHub Releases](https://github.com/inovacc/reposmerge-rs/releases/tag/v1.0.0) page.

**From source** — requires a Rust toolchain (edition 2021, MSRV 1.74):

```bash
cargo build --release        # binary at target/release/reposmerge
cargo install --path .       # install into ~/.cargo/bin
```

## Usage

### scan — discover, fingerprint, group (read-only)

```bash
reposmerge scan --roots <dir> [--roots <dir2> ...] --out . [--dest ./canonical] [--workers N] [--include-nested]
```

Walks the root directories, fingerprints each git copy in parallel, groups them, and writes `reports/inventory.csv`, `reports/third-party.csv`, and a skeleton `reports/plan.json`. `--include-nested` also discovers repos nested inside another repo's working tree. Roots are repeatable or comma-separated.

### plan — decide A/B/C strategy per group (read-only)

```bash
reposmerge plan --out . [--dest ./canonical]
```

Reads the skeleton plan, chooses a strategy per group, rewrites `reports/plan.json`, and produces `reports/divergence.md`.

### apply — execute the plan (dry-run unless `--confirm`)

```bash
reposmerge apply --plan reports/plan.json --dest ./canonical [--out .] [--confirm] [--include-generated]
```

Validates the reachability proof, then consolidates. **Dry-run by default** — pass `--confirm` to write. Generated directories (`node_modules`, `vendor`, `dist`, `.next`, `build`, `.gradle`, `target`, `__pycache__`) are excluded unless `--include-generated`. A confirmed run also writes `reports/checksums.sha256` and runs a post-apply physical proof.

### verify — prove no commit was lost

```bash
reposmerge verify --plan reports/plan.json [--physical]
```

Re-runs the static reachability proof; `--physical` additionally queries the real consolidated repos (run after `apply --confirm`).

### Example flow

```bash
reposmerge scan   --roots ~/projects --roots /mnt/backup/code --out .
reposmerge plan   --dest ./canonical --out .
reposmerge verify --plan reports/plan.json
reposmerge apply  --plan reports/plan.json --dest ./canonical --confirm
```

## Project structure

```
reposmerge-rs/
├── Cargo.toml
├── src/
│   ├── main.rs        # CLI (clap): scan / plan / apply / verify
│   ├── lib.rs         # module declarations (dependency order)
│   ├── model.rs       # core types + serde (Copy, Group, Plan, Decision, ...)
│   ├── gitx.rs        # git Runner trait (ExecRunner + Fake)
│   ├── fingerprint.rs # compute a Copy's fingerprint from git output
│   ├── group.rs       # group copies into logical repos
│   ├── discover.rs    # walk roots, find repos, classify in-scope/third-party
│   ├── report.rs      # byte-exact report artifacts (CSV, plan.json, markdown)
│   ├── safety.rs      # reachability proofs + atomic tree copy + tree hash
│   ├── strategy.rs    # A/B/C decision per group
│   ├── consolidate.rs # orchestration core: execute a Plan
│   └── app.rs         # inert mantle-config shim (framework boundary)
├── tests/
│   ├── e2e.rs         # real-git full-pipeline integration test
│   └── golden/        # byte-exact golden fixtures (plan.json, divergence.md, MANIFEST.md)
├── PORT-TRACK.md      # port parity ledger
├── PORT-GLOSSARY.md   # shared type/naming/error decisions
└── PORT-PROVENANCE.json
```

Module dependency order: `model → gitx → fingerprint → group → discover → report → safety → strategy → consolidate → app → main`.

## Dependencies

| Crate | Why |
|-------|-----|
| `serde` (+derive) / `serde_json` | JSON (de)serialization parity for the `report` module |
| `chrono` (serde) | RFC3339/calendar time parity for commit timestamps and dir mtimes |
| `csv` | `inventory.csv` / `third-party.csv` output (LF terminator to match Go) |
| `sha2` / `hex` | Sha256 source-disc digest and hex encoding |
| `walkdir` | recursive walk with prune/skip-subtree control (matches Go `filepath.WalkDir`) |
| `clap` (derive) | CLI argument parsing (Go cobra → Rust clap) |

Each addition is justified in [PORT-TRACK.md](PORT-TRACK.md) under "Dependencies added".

## Provenance

This crate is a faithful 1:1 Rust port of `github.com/inovacc/reposmerge`, signed to Go source commit `479a7c58` in `PORT-PROVENANCE.json`. Behavior is identical to the Go original; report artifacts are byte-parity audited. See [PORT-TRACK.md](PORT-TRACK.md) for the module/test/dependency/deviation ledger and [PORT-GLOSSARY.md](PORT-GLOSSARY.md) for type/naming/error decisions.

## License

BSD-3-Clause — see [LICENSE](LICENSE). Copyright (c) 2026 dyammarcano.
