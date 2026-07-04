# muldex

This project is a focused lab for understanding and reducing Codex CLI spinning, focus loss, and runaway context growth.

It is not a full Codex replacement. The first goal is to isolate runtime behaviors that appear to create self-sustaining loops:

- repeated hook-based context injection
- repeated tool-driven follow-up
- mailbox or agent wakeups with `trigger_turn = true`
- compaction that reduces context size but does not break the continue loop

The project is Rust-first. If we adopt external agent frameworks such as Agently, they should enter through a narrow sidecar or subprocess boundary rather than replacing the Rust runtime wholesale.

`muldex` should preserve Codex's interface style and operator habits as much as possible. Runtime and orchestration changes are allowed; unnecessary UI/UX divergence is not.

The name `muldex` also implies a multimodal direction. Audio and video should eventually be able to enter working context through bounded derived artifacts and summaries rather than raw media injection.

## Why this exists

The upstream `openai/codex` repository is large and tightly integrated. We want a smaller place to:

- write down runtime hypotheses clearly
- prototype guardrails and instrumentation
- decide which changes belong in an upstream fork

## Initial focus

1. Map the core continue loop in `codex-core`
2. Identify where repeated follow-up is driven by model output versus scheduler state
3. Prototype hard and soft guards for:
   - duplicate hook context injection
   - repeated trigger-turn wakeups
   - compaction-without-progress loops

## Current build direction

`muldex` will stay Rust-first.

Planned system shape:

- Rust kernel for CLI, persistence, tool execution, sandbox, token accounting, telemetry, and scheduling
- orchestration boundary with explicit continue/stop and wakeup decisions
- optional external runtime integration such as Agently through a sidecar or subprocess protocol

The first implementation goal is not a full Codex replacement. It is to define the orchestration boundary precisely enough that we can move continuation control out of the current reactive loop.

## Relationship to upstream

- `muldex`: design and experiment layer
- `openai-codex-fork`: eventual implementation target for upstream-based changes
