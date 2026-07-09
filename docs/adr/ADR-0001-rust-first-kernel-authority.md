# ADR-0001: Rust-First Kernel Authority

## Status

Accepted

## Context

`muldex` exists to move continuation control, long-task governance, and runtime safety semantics out of an implicit reactive loop.

Several possible implementations could support that goal:

- replace the whole runtime with an external framework
- let an external agent runtime own local execution and persistence
- keep Rust as the owner of local authority and use external runtimes only as bounded helpers

The project also aims to stay close to Codex operator expectations around sandboxing, approvals, and terminal-first workflow.

## Decision

`muldex` is Rust-first, and the Rust kernel remains the owner of:

- local execution authority
- sandbox semantics
- approval and escalation state
- session and runtime state
- capability registry and routing boundaries
- multimodal artifact boundaries

External agent frameworks may assist with planning or orchestration, but they do not own the kernel.

## Consequences

Positive:

- kernel-level safety semantics remain explicit and reviewable
- local execution and persistence do not become hidden inside another runtime
- Codex-like sandbox and approval compatibility remains achievable
- multimodal and capability routing remain governed by Rust-side contracts

Negative:

- more runtime infrastructure must be built in Rust
- external frameworks cannot be dropped in as full replacements
- integration work moves toward sidecars and adapters instead of wholesale adoption

## Rejected alternatives

### Replace the kernel with an external agent runtime

Rejected because it would give up too much control over sandbox, persistence, and local execution authority.

### Let external orchestration own approval and execution state

Rejected because approval and escalation semantics are part of the trusted runtime boundary, not optional planner behavior.
