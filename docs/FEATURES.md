# Features

_Generated: 2026-07-22_

Feature status for `reposmerge` v1.0.0. This is a dated snapshot, not a living contract.

## Completed (shipped in v1.0.0)

- **Four-stage CLI pipeline** — `scan → plan → apply → verify`, each read-only on sources until an explicit `apply --confirm`.
- **A/B/C strategy decision per group** — A (richest-wins + quarantine for groups with a known remote), B (union-all-history for local groups with shared lineage), C (verbatim snapshot for irreconcilable name collisions).
- **Commit-reachability proofs** — a static plan proof runs before any write and aborts if any source commit would become unreachable; an optional physical proof queries the real consolidated repos after apply (`verify --physical`).
- **Resilient atomic CopyTree** — each repo is copied to a temp dir then renamed into place with rollback; a failure never leaves a partial tree. Unreadable/locked files are skipped and reported in `MANIFEST.md`.
- **Idempotent apply** — a repo whose destination already matches by content hash is skipped, so re-runs are safe.
- **TreeHash integrity** — a confirmed run writes `reports/checksums.sha256` over the consolidated tree (same-platform deterministic).
- **Byte-parity reports** — `plan.json`, `inventory.csv`, `third-party.csv`, `divergence.md` are byte-identical to the Go original.
- **Nested-repo discovery** — `scan --include-nested` finds repos nested inside another repo's working tree.
- **Bounded-parallel fingerprinting** — `scan --workers N` fingerprints copies across a bounded worker pool (default `NumCPU*2`, clamped 1–16).

## Proposed (natural extensions, not committed)

- **Config-file support** — the `--config/-c` flag is currently an accepted no-op (mantle-parity). Wire it to a real config loader only if a genuine config need arises.
- **Cross-platform TreeHash normalization** — normalize file-mode bits so tree hashes match across OSes, only if cross-platform hash equality becomes a requirement.
