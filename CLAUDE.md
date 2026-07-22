# CLAUDE.md
<!-- rev:001 (RFC 3339) 2026-07-22T19:46:58Z -->

Claude Code entry point. Canonical cross-tool agent instructions live in **AGENTS.md** (imported below). Keep shared rules there — do not duplicate them here.

@AGENTS.md

## Claude-Code-only

- This crate is a **parity-constrained port**: it is a faithful 1:1 Rust port of `github.com/inovacc/reposmerge`. Behavior and output must stay byte-identical to the Go source.
- Before touching any module, read the port ledgers: `PORT-TRACK.md` (per-module parity status, dependency justifications, documented deviations) and `PORT-GLOSSARY.md` (shared type/naming/error decisions). `PORT-PROVENANCE.json` records the signed Go source commit.
