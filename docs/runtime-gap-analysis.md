# Runtime Gap Analysis

## Purpose

Record the highest-value runtime gaps in `muldex` based on comparisons against existing coding-agent runtimes, especially `jcode`, `Codex`, and Claude Code.

This document is intentionally narrower than the broader architecture docs. It is focused on runtime control and long-running task behavior.

## Current strengths

`muldex` already has useful core direction in place:

- Rust-first orchestration boundary
- explicit continuation and safety-oriented protocol types
- deterministic reasoning harness decisions
- upstream snapshot adapters for real Codex state
- a narrow Agently sidecar seam rather than full kernel replacement

These are good foundations, but they do not yet form a full runtime.

## Highest-priority gaps

### 1. No dedicated runtime kernel layer yet

Current state:

- runtime-like behavior is still implied through core protocol and CLI harness flows

Why this matters:

- long-running agents need an owner for lifecycle, wakeup, interruption, and resume behavior

Required direction:

- introduce a runtime-focused layer responsible for execution control primitives

### 2. No soft-interrupt model

Current state:

- continuation decisions are structured, but there is not yet a first-class mechanism for queueing and safely injecting new events into ongoing work

Why this matters:

- long-running work cannot rely only on stop or continue
- approvals, user input, and sidecar outputs often need delayed injection at safe points

Required direction:

- add safe-point injection semantics and queued interrupt state

### 3. No pending-approval runtime state

Current state:

- escalation and prohibition logic exists, but approval requests are not yet modeled as persistent runtime objects

Why this matters:

- a long-running task should be able to defer a risky action without collapsing the whole run

Required direction:

- add persistent approval request and post-decision continuation policy types

### 4. No explicit cycle or scheduled execution model

Current state:

- the current model is still closest to foreground interactive evaluation of a request or snapshot

Why this matters:

- anti-spin logic becomes more reliable when the runtime can move work across bounded cycles rather than stretching one turn indefinitely

Required direction:

- define execution modes and wakeup policies for scheduled and background continuation

### 5. Continuity state is still too thin for durable learning

Current state:

- snapshot adapters and continuation signals exist, but retained state is not yet rich enough for durable, self-correcting long tasks

Why this matters:

- long-running systems need provenance, confidence, and replacement semantics to avoid stale or contradictory retained state

Required direction:

- enrich retained state with provenance, confidence, supersession, contradiction, and bounded evidence references

### 6. No structured run report contract

Current state:

- decisions are inspectable, but there is not yet a stable operator-facing summary object for what happened during a run or cycle

Why this matters:

- long-running agent governance needs summaries, not only instantaneous decisions

Required direction:

- add `RunReport` and `CycleSummary` style protocol types

## Suggested near-term crate shape

`muldex` does not need `jcode`-level crate granularity yet. It does need sharper layering.

Recommended shape:

- `muldex-core`
  - protocol types
  - snapshot adapters
  - capability descriptors
  - reasoning harness policy
- `muldex-runtime`
  - lifecycle state
  - queued interrupts
  - approvals and wakeup state
  - checkpoint and resume control
  - cycle reports
- `muldex-cli`
  - operator entrypoint
  - debug commands
  - runtime control and inspection commands

## Recommended work order

### Step 1

Add runtime primitives before adding more policy sophistication.

### Step 2

Add approval-state and reporting contracts before attempting unattended long-run execution.

### Step 3

Add scheduled-cycle semantics before experimenting with autonomous continuation feedback into the upstream runtime.

### Step 4

Only after the above, expand memory-like retained state and richer sidecar coordination.

## Summary

`muldex` already has the beginnings of a governance kernel.

The next important shift is from protocol-only reasoning to runtime-owned execution semantics.
