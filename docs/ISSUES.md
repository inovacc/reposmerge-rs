# Known Issues & Limitations

_Generated: 2026-07-22_

These are documented, intentional limitations of the faithful port — not defects. See `PORT-TRACK.md` for the full parity ledger.

## Smart-match grouping — deliberate deviation from the Go oracle (default-on)

The Go original keys a group by remote URL when present, else `name+lineage`. That **under-merges** two real-world cases (both false-negatives — missed duplicates, never data loss):

1. **Remote-backed vs local-only copy of the same repo** — clone once with an `origin`, copy the folder elsewhere and the remote gets stripped/changed (or you init from a zip): Go keys them `remote:<url>` and `noremote:<name>:<lineage>` → two groups for one repo.
2. **Case-variant remote URLs** — `github.com/inovacc/CaseTest` vs `…/casetest` (GitHub treats them as the same repo) → two groups.

**This port keys lineage-first** (identity = the sorted root-commit set), so every clone of a repo unifies regardless of remote presence or URL form/case; empty/uninitialized repos fall back to a case-insensitive remote key, else name. It is a **strict superset** of Go's grouping — it never splits a group Go kept, only joins ones Go wrongly split — and stays fully lossless (`verify` proves every source commit survives; deviations go to quarantine/union).

**Trade-off (accepted):** two repos that share a root commit but were meant to stay separate (a fork/template clone with a different remote) now merge into one group. Because consolidation is lossless (union/quarantine preserve every commit and branch), this is safe; the `smart_divergent_lineage_still_separate` test guards that genuinely different root commits never merge. To find scattered copies you must still pass every location in one run: `scan --roots <A> --roots <B> …`.

**Consequence for parity:** grouping output intentionally diverges from the Go tool on these cases; report *serialization/format* parity is unaffected.

## Mantle framework runtime not reimplemented

The Go original embeds `mantle/bootstrap.Base` and wires a whole framework runtime (viper config loading, otel observability, structured/redacting logger, daemon supervisor) in cobra's `PersistentPreRunE`. reposmerge's own commands never read that runtime, so per the porting rule "map a framework, don't reimplement it" it is **out of scope**. Consequences:

- Only `--config/-c` (accepted but unused, for parity) and `--version` are reproduced. **Decision (settled):** `--config` stays an intentional accepted-no-op — no config crate is pulled in — since no reposmerge command consumes a loaded config; revisit only if a real config need arises.
- The mantle global flags `--env`, `--log-level`, `--verbose/-v`, `--quiet/-q`, `--log-format`, `--log-source`, `--no-redact`, `--otel*`, and `--daemon` are **intentionally omitted**.
- `app.rs` reproduces only the inert config data of `DefaultBase()`; the struct is never read by any command.

## TreeHash is platform-dependent

The absolute value of `safety::tree_hash` depends on the platform's file mode bits: Unix uses `st_mode`, Windows synthesizes `0o444`/`0o666`. Only **same-platform determinism** is guaranteed (identical trees hash equal on the same OS) — this already matches the Go tool's behavior. Do not compare tree hashes across operating systems.

## Requires real `git` on PATH

The `tests/e2e.rs` integration test, several real-git unit tests (discover nested repos, consolidate idempotency), and all real-repo operations shell out to `git`. `git` must be on `PATH`. Pure-logic modules use the in-memory `gitx::Fake` runner and need no git.

## Windows-specific atomic-rollback test

The safety atomic-rollback test is **Windows-only** (skipped elsewhere, mirroring the Go `runtime.GOOS` skip). It forces `remove_dir_all` to fail by holding a handle open without delete-sharing to exercise the rollback path. On non-Windows platforms the rollback assertions are not exercised.

## Repository is local-only — RESOLVED (2026-07-22)

~~There is no git remote configured for this crate.~~ **Resolved:** the repository is now live at <https://github.com/inovacc/reposmerge-rs> (public, default branch `main`) with tag `v1.0.0` released. Distribution is via GitHub Releases only — the crate is not published to crates.io (`publish = false`).
