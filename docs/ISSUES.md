# Known Issues & Limitations

_Generated: 2026-07-22_

These are documented, intentional limitations of the faithful port — not defects. See `PORT-TRACK.md` for the full parity ledger.

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

## Repository is local-only

There is no git remote configured for this crate; it cannot be cloned/pushed until a remote is added (tracked in `docs/BACKLOG.md`).
