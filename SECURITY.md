# Security Policy

## Supported versions

The latest `1.x` release is supported. Older versions are not maintained.

| Version | Supported |
|---------|-----------|
| 1.x     | ✅        |
| < 1.0   | ❌        |

## Reporting a vulnerability

Please report security issues **privately** via GitHub's
[private vulnerability reporting](https://github.com/inovacc/reposmerge-rs/security/advisories/new)
rather than opening a public issue.

Include: affected version, a description, reproduction steps, and impact. You can
expect an initial acknowledgement within a few days.

## Scope notes

`reposmerge` operates on local git working trees and shells out to the system
`git` binary. It never transmits repository contents over the network. The
dependency tree is gated in CI by `cargo-deny` (advisories, licenses, sources).
