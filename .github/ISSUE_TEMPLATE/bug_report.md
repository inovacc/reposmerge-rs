---
name: Bug report
about: Report incorrect behavior or a parity divergence from the Go tool
title: "[bug] "
labels: bug
---

## Description

A clear description of what's wrong.

## Reproduction

Steps or a minimal repo layout that triggers it:

```
reposmerge scan --roots ... --out ...
reposmerge plan --out ...
reposmerge apply --plan ... [--confirm]
```

## Expected vs actual

- **Expected:** …
- **Actual:** …
- If this is a **parity divergence** from the Go tool, include both outputs.

## Environment

- OS + arch:
- `reposmerge --version`:
- `git --version`:
