# Implementation Plan

Baseline reference:

- `docs/current-baseline.md` describes the current implemented floor that future work should preserve unless intentionally revised.

## Phase 1: boundary-first skeleton

### Work item 1: Rust workspace skeleton

Deliverables:

- Cargo workspace
- one library crate for core orchestration types
- one binary crate placeholder for future CLI/runtime work

Acceptance:

- `cargo check` succeeds
- workspace layout is documented

### Work item 2: orchestration protocol types

Deliverables:

- `ContinueReason`
- `StateChangeKind`
- `ContextPressure`
- `ContinueRequest`
- `ContinueMode`
- `ContinueDecision`
- optional planner request/response types

Acceptance:

- types compile
- types derive debug, clone where appropriate, and serde traits
- unit tests cover basic serialization round trips

### Work item 3: orchestration traits

Deliverables:

- `AgentOrchestrator`
- `ContextGovernor`
- `WakeupPolicy`
- `ToolContinuationPolicy`

Acceptance:

- traits compile
- one no-op or deterministic local implementation exists
- documentation explains ownership boundaries

### Work item 4: Agently sidecar contract draft

Deliverables:

- sidecar protocol doc
- Rust-side request/response adapter traits or placeholders
- explicit note that sidecar is optional and out-of-process

Acceptance:

- protocol is documented with example JSON
- no Python dependency is required for Rust workspace to compile

### Work item 5: audio/video context ingestion boundary

Deliverables:

- media asset reference types
- bounded derived media artifact types
- operator-facing and model-facing media context envelope

Acceptance:

- audio/video enters orchestration as derived artifacts, not raw media
- protocol and data structures are documented
- later implementation can support Kimi-style workflows without rewriting continuation logic

### Work item 6: media-generation backend contract

Deliverables:

- capability descriptors for diffusion and video-generation backends
- generated-artifact lifecycle notes
- routing notes for when text models hand work to generation backends

Acceptance:

- ComfyUI- or Seedance-style backends fit into the protocol without redesigning the kernel
- generated media outputs are represented as explicit artifacts, not implicit side effects

### Work item 7: reasoning harness contract

Deliverables:

- reasoning harness request and policy types
- explicit prohibition list structure
- rationale for stop, checkpoint, self-correction, or escalation

Acceptance:

- model reasoning can be governed by explicit, reviewable constraints
- prohibited behaviors are not buried only inside one prompt template

## Phase 2: first runtime guards

This phase now also carries a runtime-kernel direction sharpened by comparison with `jcode`.

The next missing ingredients are not only more policies, but explicit runtime control primitives:

- soft interrupts and safe-point injection
- pending approval state and post-approval continuation behavior
- execution-mode descriptors for interactive, resumed, and scheduled work
- structured run and cycle summaries

### Work item 8: duplicate hook-injection suppression

Deliverables:

- local policy API for dedupe decisions
- integration plan for upstream fork patch

Acceptance:

- repeated identical injected contexts can be suppressed by policy
- rationale is observable in logs or returned decisions

### Work item 9: trigger-turn downgrade policy

Deliverables:

- explicit wakeup decision path
- policy shape that can downgrade `trigger_turn` to queue-only

Acceptance:

- repeated wakeups with no meaningful state change can be rejected or downgraded in tests

### Work item 10: compaction-without-progress detector

Deliverables:

- loop detector state shape
- policy signal for handoff or forced stop

Acceptance:

- repeated compact-without-state-change sequences produce a structured escalation decision

### Work item 11: Kimi-style audio/video context workflow

Deliverables:

- import local or referenced audio/video assets
- derive transcript, segment summaries, and evidence summaries
- for video, derive keyframes and shot or segment summaries
- inject only bounded references and summaries into orchestration and model context

Acceptance:

- operator can attach audio/video without changing Codex-style interaction rhythm
- model sees bounded summaries and references instead of raw media payloads
- segment selection is inspectable and replayable

## Fork workstream

The upstream fork should only start taking behavior patches after phase 1 types and traits are stable enough.

Initial target files:

- `codex-rs/core/src/hook_runtime.rs`
- `codex-rs/hooks/src/events/user_prompt_submit.rs`
- `codex-rs/core/src/tasks/regular.rs`
- `codex-rs/core/src/tasks/mod.rs`
- `codex-rs/core/src/session/turn.rs`
- `codex-rs/core/src/tools/handlers/multi_agents_v2/message_tool.rs`

## Current interactive shell product direction

The current plan should now be read together with:

- `docs/codex-tui-compatibility-matrix.md`
- `docs/interactive-shell-guide.md`
- `docs/interactive-shell-release-checklist.md`

The practical product direction is no longer only a runtime/admin CLI.

Current shell-facing implementation work now includes:

- default `muldex` entry into an interactive shell
- persisted shell sessions and resume flows
- runtime-backed slash command compatibility slice
- prompt history, reverse history search, and slash picker behavior
- Windows Terminal redraw guardrails intended to avoid the "gets slower over time" failure mode common in terminal agent shells

The next product-critical shell requirement is operator-managed LLM endpoint setup.

Near-term shell planning should therefore also include:

- persisted user-scoped `llm-router` configuration
- shell-native configuration flow for host, port, and API key
- startup-time detection of missing router configuration

Near-term implementation planning should therefore treat the interactive shell as a first-class product surface rather than only an optional wrapper around daemon and runtime commands.
