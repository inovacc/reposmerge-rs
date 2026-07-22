# Architecture
<!-- rev:001 (RFC 3339) 2026-07-22T19:46:58Z -->

`reposmerge` is a faithful 1:1 Rust port of `github.com/inovacc/reposmerge`. The binary (`src/main.rs`, clap) dispatches four subcommands over a library whose modules are declared in strict dependency order (`src/lib.rs`).

## System overview

The CLI subcommands drive the library modules. Each module depends only on modules earlier in the port order.

```mermaid
flowchart TB
    subgraph CLI["src/main.rs (clap)"]
        scan[scan]
        plan[plan]
        apply[apply]
        verify[verify]
    end

    model[model<br/>core types + serde]
    gitx[gitx<br/>git Runner trait]
    fingerprint[fingerprint<br/>compute fingerprint]
    group[group<br/>group copies]
    discover[discover<br/>walk + classify]
    report[report<br/>byte-exact artifacts]
    safety[safety<br/>reachability + atomic copy]
    strategy[strategy<br/>A/B/C decision]
    consolidate[consolidate<br/>orchestration core]

    scan -->|discover| discover
    scan -->|fingerprint| fingerprint
    scan -->|group| group
    scan -->|write reports| report
    plan -->|load/rewrite plan| report
    plan -->|decide| strategy
    apply -->|reachability proof| safety
    apply -->|execute| consolidate
    apply -->|manifest + checksums| report
    verify -->|reachability proof| safety

    fingerprint --> gitx
    fingerprint --> model
    discover --> gitx
    group --> model
    report --> model
    safety --> gitx
    safety --> model
    strategy --> model
    consolidate --> safety
    consolidate --> gitx
    consolidate --> model
```

## Pipeline sequence (scan → plan → apply → verify)

```mermaid
sequenceDiagram
    actor User
    participant CLI as main.rs
    participant D as discover
    participant F as fingerprint
    participant G as group
    participant S as strategy
    participant Sa as safety
    participant C as consolidate
    participant R as report

    User->>CLI: scan --roots ...
    CLI->>D: discover(roots, scope, excludes, nested)
    D-->>CLI: in-scope copies + third-party
    CLI->>F: compute(copy) per copy (parallel)
    CLI->>G: build(copies) -> groups
    CLI->>R: write_inventory + write_plan (skeleton)

    User->>CLI: plan --dest ./canonical
    CLI->>R: load_plan
    CLI->>S: decide(group, dest) per group -> A/B/C
    CLI->>R: write_plan (+ divergence.md)

    User->>CLI: verify --plan reports/plan.json
    CLI->>Sa: reachability_proof(plan)
    Sa-->>CLI: violations (empty = OK)

    User->>CLI: apply --confirm
    CLI->>Sa: reachability_proof (abort if any loss)
    CLI->>C: apply(runner, plan, options)
    C->>Sa: copy_tree_atomic + tree_hash (idempotency)
    C-->>CLI: ApplyResult (copied/quarantined/unioned)
    CLI->>Sa: physical_reachability (post-apply proof)
    CLI->>R: write_manifest + write_checksums
```

## A/B/C strategy decision

`strategy::decide` picks a per-group consolidation strategy based on whether the group has a known remote and whether its local copies share lineage.

```mermaid
flowchart TD
    start([Group]) --> hasRemote{Group has a<br/>known remote?}
    hasRemote -->|yes| A[Strategy A<br/>richest-wins + quarantine<br/>keep richest canonical,<br/>quarantine divergent history]
    hasRemote -->|no| lineage{Local copies share<br/>lineage / can union?}
    lineage -->|yes| B[Strategy B<br/>union-all-history<br/>union every copy's branches<br/>into the canonical repo]
    lineage -->|no / collision| C[Strategy C<br/>verbatim snapshot<br/>name collision that cannot<br/>be auto-reconciled]
```

## Module responsibilities

| Module | Responsibility |
|--------|----------------|
| `model` | Core serde types (Copy, Group, Plan, Decision, ApplyResult, StrategyKind, ...) with exact Go JSON keys |
| `gitx` | `Runner` trait wrapping `git` (ExecRunner + Fake for tests) |
| `fingerprint` | Fill a Copy's fingerprint (head, commit counts, branches, lineage) from git output |
| `group` | Group copies into logical repos in first-seen order |
| `discover` | Walk roots, find repos, classify in-scope vs third-party (with optional nested discovery) |
| `report` | Emit byte-exact artifacts: `inventory.csv`, `third-party.csv`, `plan.json`, `divergence.md`, `MANIFEST.md`, `checksums.sha256` |
| `safety` | Static + physical reachability proofs, atomic tree copy with rollback, content tree hash |
| `strategy` | Decide A/B/C per group |
| `consolidate` | Orchestration core: execute a Plan (copy / quarantine / union), idempotent + atomic |
| `app` | Inert mantle-config shim (framework boundary — runtime not reimplemented) |
