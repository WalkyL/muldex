# TODO

## Delivery model

### Coding pass

- target model intent: `gpt-5.4-mini`
- goal: move quickly on bounded implementation slices
- output: compiling code, narrow tests, explicit tradeoffs

### Acceptance pass

- target model intent: `gpt-5.4`
- goal: challenge boundaries, missing fields, ownership mistakes, and weak acceptance criteria
- output: review findings, residual risk, and next required changes

## Phase 1 implementation items

### TODO-000: Codex UX compatibility contract

Scope:

- document which parts of Codex interaction style are compatibility targets
- treat runtime replacement and UI replacement as separate decisions

Acceptance:

- docs explicitly state that Codex interface style and operator habits should be preserved by default
- later implementation items do not silently assume a brand-new interaction model

Status:

- done in current slice

### TODO-000A: Audio/video context capability contract

Scope:

- document how audio and video enter the system
- require bounded derived artifacts instead of raw media context injection

Acceptance:

- docs explicitly state that audio/video must become inspectable derived artifacts before model use
- later implementation items do not assume raw media is directly stuffed into prompt context

Status:

- done in current slice

### TODO-001: Rust workspace skeleton

Scope:

- create Cargo workspace
- create `muldex-core`
- create `muldex-cli`

Acceptance:

- `cargo check` passes at workspace root
- repository layout matches documentation

Status:

- done in current slice

### TODO-002: Core orchestration protocol

Scope:

- define continuation request/decision types
- define planner request/response types
- ensure serde compatibility

Acceptance:

- protocol types compile
- JSON round-trip test exists for at least one top-level request

Status:

- done in current slice

### TODO-003: Orchestration traits

Scope:

- define `AgentOrchestrator`
- define `ContextGovernor`
- define `WakeupPolicy`
- define `ToolContinuationPolicy`

Acceptance:

- traits compile
- ownership boundaries are documented

Status:

- done in current slice

### TODO-004: Deterministic local orchestrator

Scope:

- provide one local implementation for continuation decisions
- prove the boundary can host policy without any external runtime yet

Acceptance:

- implementation compiles
- behavior is explicit and reviewable

Status:

- done in current slice

### TODO-005: Agently sidecar contract draft

Scope:

- specify request/response JSON shape
- define failure semantics and timeout expectations
- keep Rust build free of Python dependency

Acceptance:

- contract doc exists
- no Python installation is required for current Rust workspace to build

Status:

- partially done in docs, not yet encoded as Rust adapter types

### TODO-006: Audio/video context ingestion contract

Scope:

- define media asset reference types
- define derived media artifact types
- define operator-facing and model-facing media context envelope
- define ASR and alignment capability placeholders

Acceptance:

- audio/video context enters the system through bounded derived artifacts
- docs make clear that raw media is not directly injected into model context
- ASR and alignment are treated as first-class multimodal capability surfaces

Status:

- documented and encoded as initial Rust protocol types

### TODO-006A: Agent mode, subagent, and surface mobility contract

Scope:

- define agent-mode descriptors such as plan/build style roles
- define subagent capability descriptors
- define session-surface descriptors for terminal, desktop, and remote or detached use

Acceptance:

- orchestration can reason about mode, subagent availability, and surface constraints explicitly
- these dimensions are not left to hidden UI or runtime assumptions

Status:

- documented, not yet encoded as Rust types

### TODO-006B: Generative media backend contract

Scope:

- define capability descriptors for ComfyUI, Seedance, and similar generation backends
- define generated artifact expectations and reviewable output references
- define how generation backends fit model routing and orchestration decisions

Acceptance:

- generation backends are explicit protocol-level capability providers
- generated outputs are represented as artifacts with inspectable provenance

Status:

- documented, not yet encoded as dedicated Rust capability descriptors in policy/runtime layers

### TODO-006C: ASR and alignment capability contract

Scope:

- define ASR backend descriptors
- define alignment backend descriptors
- ensure media and hyperframe protocol can reference alignment outputs cleanly

Acceptance:

- ASR and alignment are explicit protocol-level capability providers
- media-derived reasoning can depend on them without new kernel ownership changes

Status:

- documented and encoded as initial Rust protocol types

### TODO-006D: Long-running autonomous execution contract

Scope:

- define progress-reporting structures
- define checkpoint/resume structures
- define self-correction and recovery semantics
- define anti-spin escalation semantics

Acceptance:

- long tasks can represent progress and recovery without hidden runtime assumptions
- self-correction is explicit and bounded
- no-progress repetition can be distinguished from useful continuation

Status:

- documented and encoded as initial Rust protocol types

### TODO-006E: Post-compaction and runtime mode state contract

Scope:

- define post-compaction runtime state
- define runtime mode state
- define invoked-skill preservation state

Acceptance:

- post-compaction follow-up behavior can be reasoned about explicitly
- mode transitions and invoked skills are not lost across long-running execution logic

Status:

- documented and encoded as initial Rust protocol types

### TODO-006E: Reasoning harness and prohibition contract

Scope:

- define harness request structures
- define explicit prohibition and escalation structures
- define how harness policy is attached to reasoning-intensive turns

Acceptance:

- models can be asked to reason under an explicit contract
- prohibited behaviors are represented in code and docs, not only prompt prose

Status:

- documented, not yet encoded as dedicated Rust protocol types

## Phase 2 target items

### TODO-008: Hook injection dedupe policy

Scope:

- add kernel-facing signal for duplicate injection detection
- define policy output for suppressing repeated injected context

Acceptance:

- repeated identical injected guidance can be suppressed by policy
- decision is observable in logs or structured results

### TODO-009: Trigger-turn downgrade policy

Scope:

- define policy path from wakeup candidate to queue-only downgrade

Acceptance:

- repeated wakeups with no meaningful state change can be rejected or downgraded

### TODO-010: Compaction-without-progress detector

Scope:

- detect repeated compact cycles without enough state change
- emit handoff or stop recommendation

Acceptance:

- repeated compact-without-progress sequences produce a structured escalation signal

### TODO-011: Kimi-style audio/video context workflow

Scope:

- accept audio/video assets from terminal-first workflow
- derive transcript, keyframes for video, segment summaries, and evidence summaries
- inject only bounded references and summaries into orchestration and model context

Acceptance:

- operator workflow remains close to Codex habits
- selected segments and derived artifacts are inspectable
- media-derived context obeys context budget rules

## Acceptance checklist for current slice

- workspace exists
- architecture docs exist
- implementation plan exists
- acceptance criteria exist
- protocol types compile
- orchestration traits compile
- local deterministic implementation compiles
- `cargo check` passes
- binary supports a real workspace invocation path
