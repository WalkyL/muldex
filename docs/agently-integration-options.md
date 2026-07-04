# Agently Integration Options

## Constraint

`Agently` is a Python framework, not a Rust crate.

That means `muldex` cannot stay a pure Rust in-process runtime if it wants to reuse Agently directly.

## Recommended direction

Keep `muldex` as a Rust-first project and integrate Agently out of process.

Recommended shape:

1. Rust remains the product shell and primary runtime.
2. Agently runs as a sidecar service or subprocess.
3. Rust talks to Agently over a narrow protocol.
4. Only selected orchestration or planning flows are delegated to Agently.

This keeps the product identity and systems code in Rust while letting us adopt a stronger agent framework where it helps.

## Integration models

### A. Sidecar service

- Run Agently as a local HTTP service.
- Rust sends structured task payloads and receives structured plans, actions, or workflow results.
- Best when we want a stable boundary and independent iteration.

Good for:

- planning
- workflow orchestration
- multi-agent policy experiments
- structured output normalization

### B. Managed subprocess

- Rust launches a Python worker process when needed.
- Exchange JSON over stdio or a local socket.
- Smaller deployment surface than a long-lived HTTP service.

Good for:

- local development
- narrow experiments
- temporary bridge while validating value

### C. MCP-style capability bridge

- Expose Agently-backed capabilities as tools or services.
- Rust keeps the main turn loop and selectively calls Agently-backed operations.

Good for:

- incremental adoption
- keeping Codex-compatible tool boundaries

## What should stay in Rust

- CLI and TUI shell
- session persistence
- local file and process control
- sandbox and approval model
- compact/token accounting
- thread and mailbox scheduling
- telemetry and diagnostics

These are the parts where direct control and low operational complexity matter most.

## What can move to Agently

- plan generation
- multi-step workflow orchestration
- structured action planning
- model-facing skill or workflow policy
- evaluator/reviser loops
- selected multi-agent collaboration patterns

## What not to do first

- Do not rewrite the full Codex runtime around Agently immediately.
- Do not put Python deep inside the Rust hot path for every turn.
- Do not mix Rust-side and Python-side scheduling authority before the boundary is explicit.

## First practical slice

Start with one delegated capability only:

- Rust owns the session and turn loop.
- Rust calls an Agently worker for "plan this task" or "decide next action".
- The returned plan is logged, bounded, and replayable from Rust.

If that works, expand into:

- workflow orchestration
- follow-up suppression
- multi-agent coordination policies

## Decision

For `muldex`, the best current path is:

- Rust-first product
- Agently as sidecar/subprocess
- explicit protocol boundary
- selective delegation instead of full runtime replacement
