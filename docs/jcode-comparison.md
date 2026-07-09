# Jcode Comparison

## Purpose

Use `jcode` as a reference for long-running coding-agent runtime design so `muldex` does not stay trapped at the level of one-shot CLI continuation decisions.

This is not a product-clone goal. It is a runtime-boundary and protocol-gap audit.

## What `jcode` is actually useful for

`jcode` is most valuable to `muldex` as a working example of a coding-agent runtime that treats the following as first-class concerns:

- long-lived sessions
- server-owned runtime state
- reconnect and reload
- multi-session coordination
- asynchronous memory and background work
- approval and notification boundaries
- provider, tool, browser, and MCP adapters

Its strongest lessons are below the UI layer.

## Runtime mechanisms worth borrowing

### 1. Separate runtime kernel from app shell

Why it matters:

- `jcode` has a dedicated runtime-oriented crate for interruption primitives instead of burying them inside the main app.
- Long-running control semantics become easier to test when they live in a small boundary-focused layer.

Required `muldex` response:

- introduce an explicit runtime layer alongside `muldex-core` and `muldex-cli`
- keep turn-control, interruption, continuation, and lifecycle semantics in that layer

### 2. Soft interrupts instead of hard turn cancellation

Why it matters:

- `jcode` uses queued mid-turn interruption semantics rather than treating every new event as a full restart.
- This is a better fit for long-running coding work where user input, system events, or approvals may arrive while useful work is already in progress.

Required `muldex` response:

- add a soft-interrupt or safe-injection concept to runtime control
- distinguish between stop, continue, and inject-at-safe-point decisions

### 3. Server-owned sessions and reconnectable clients

Why it matters:

- `jcode` treats the server as the owner of sessions, provider connections, and shared tool state.
- Clients are attachable views rather than session owners.

Required `muldex` response:

- evolve toward a daemon or runtime process that owns session state
- keep CLI and future surfaces as clients or control entrypoints rather than the sole owner of task state

### 4. Background cycles as a real execution mode

Why it matters:

- `jcode` already has runner and scheduler substrate for background cycles even where higher-level ambient behaviors are still evolving.
- This is a better model for long tasks than pretending one foreground turn can safely stretch forever.

Required `muldex` response:

- add explicit execution modes for foreground, resumed, scheduled, and background work
- make checkpoint and wakeup semantics cycle-aware rather than turn-only

### 5. Asynchronous memory and delayed injection

Why it matters:

- `jcode` memory retrieval is designed to be non-blocking and one-turn-behind rather than stalling the main loop.
- Retrieved memories are verified and consolidated instead of injected blindly.

Required `muldex` response:

- keep continuity and future memory retrieval off the critical path
- prefer delayed bounded injection over synchronous expansion of the active turn
- track provenance and confidence for retained state

### 6. Approval as a queueable runtime state

Why it matters:

- `jcode`'s safety direction treats permission requests as persistent runtime objects, not just prompt instructions.
- Long-running agents need to wait, defer, or move on without losing continuity.

Required `muldex` response:

- add protocol types for approval requests, pending approval state, and post-approval continuation behavior
- separate approval-needed from stop-needed

### 7. Capability surfaces normalized behind adapters

Why it matters:

- `jcode` exposes browser, MCP, providers, and other integrations through normalized runtime surfaces.
- This avoids baking backend quirks into the core loop.

Required `muldex` response:

- keep Agently, MCP, multimodal backends, and future browser-like tools behind explicit capability descriptors and adapter seams
- preserve the Rust kernel as the owner of orchestration policy

### 8. Multi-agent coordination as explicit state, not process folklore

Why it matters:

- `jcode` swarm design tracks ownership, lifecycle, report-back, and message paths explicitly.
- Even if `muldex` does not implement swarms soon, the state model is a useful reference.

Required `muldex` response:

- if multi-agent support appears later, require explicit task ownership, lifecycle state, and completion reports
- avoid naive child-process spawning with implicit coordination

## What to treat cautiously

### 1. Feature ambition exceeds uniformly mature implementation

`jcode` has several areas where the design direction is clear but the complete system should not be assumed production-settled:

- safety system
- higher-level ambient behavior
- browser provider protocol document
- some multi-session client design notes

For `muldex`, this means borrow shape and boundary lessons first, not implementation confidence.

### 2. The crate split is sharper than `muldex` currently needs

`jcode` uses many focused crates. That proves the boundaries matter, but `muldex` should not copy the exact granularity yet.

For `muldex`, this means:

- borrow the layering logic
- do not prematurely explode the workspace into many tiny crates

### 3. UI depth is not the current priority

`jcode` invests heavily in rendering, widgets, side panels, and terminal behavior.

For `muldex`, this is not current leverage. Runtime correctness matters more than operator-surface polish.

## Concrete `muldex` gaps after this comparison

The following gaps are the most important ones highlighted by `jcode`.

### P0

- no explicit runtime crate for turn lifecycle and interruption semantics
- no soft-interrupt or safe-injection model
- no persistent approval-request state model
- no structured run summary or cycle report contract

### P1

- continuity state is not yet rich in provenance, confidence, or replacement relationships
- execution mode is still too foreground and turn-centric
- wakeup and continuation are not yet modeled as scheduled or background cycles
- capability adapters are present in direction but not yet unified by runtime-facing descriptors

### P2

- no explicit multi-agent ownership and completion-report contract
- no reconnect or reload architecture yet beyond current CLI invocation shape
- no async memory or continuity retrieval subsystem beyond current snapshot-driven evaluation

## Recommended implementation order

### 1. Runtime kernel boundary

Add a runtime-focused layer responsible for:

- turn lifecycle
- soft interrupts
- pending approvals
- wakeup scheduling
- checkpoint and resume boundaries

### 2. Approval and report protocol

Add explicit types for:

- `PermissionRequest`
- `PermissionDecision`
- `PendingApprovalState`
- `RunReport`
- `CycleSummary`

### 3. Execution-mode expansion

Add explicit runtime mode descriptors so decisions can differ between:

- interactive foreground work
- resumed work
- scheduled background work
- delegated or sidecar-assisted work

### 4. Continuity-state enrichment

Extend retained state with:

- provenance
- confidence
- supersession
- contradiction markers
- bounded evidence references

### 5. Adapter unification

Keep Agently, MCP, and later multimodal or browser-like integrations behind the same runtime-owned capability and orchestration boundary.

## Summary

`jcode` reinforces a core architectural point for `muldex`:

The real leverage is in runtime design, not prompt cleverness.

If `muldex` wants to prevent long-task spin while still doing useful programming work, it needs explicit runtime semantics for interruption, approval, wakeup, reporting, and cycle-based continuation.
