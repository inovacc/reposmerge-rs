## What & why

Describe the change and the motivation.

## Checklist

- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` passes (incl. the real-git e2e; `git` on PATH)
- [ ] Preserves **byte-parity** with the Go tool (or explains/justifies any deviation in `PORT-TRACK.md`)
- [ ] Docs updated if behavior/flags changed
- [ ] Conventional commit messages; no AI attribution
