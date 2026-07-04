# Architecture Boundary

## Objective

Move continuation control out of an implicit reactive loop and into an explicit orchestration decision layer.

## Layer model

### 1. Rust kernel

The Rust kernel owns:

- CLI and TUI shell
- session and thread persistence
- local tool execution
- sandbox and approvals
- token accounting and compaction accounting
- mailbox storage and delivery
- telemetry, tracing, and replayability
- local media ingestion boundaries for audio, video, and hyperframe assets
- model routing and capability awareness
- capability registry for MCP, skills, and Agent Data Protocol surfaces

The Rust kernel does not decide continuation from raw signals alone.
It also should not inject raw long-form media into model context. Media must be transformed into bounded artifacts, hyperframes, and references first.
The kernel should know enough about model capabilities to route text, image, audio, and video-derived tasks to an appropriate model or provider.

### 2. Orchestration boundary

The orchestration boundary translates raw runtime signals into explicit requests for a continuation decision.

Examples of raw signals:

- model asked for follow-up
- tool result completed
- pending input exists
- trigger-turn mailbox item exists
- context pressure is high
- repeated compaction has occurred

The orchestration boundary returns explicit decisions:

- continue now
- continue in next turn
- queue only
- stop
- handoff

It may also request a model switch or a model class for the next step.

### 3. External agent runtime

An external agent runtime such as Agently may be used for:

- planning
- workflow orchestration
- evaluator/reviser policies
- multi-agent coordination policies
- structured next-action selection

It must not directly own local execution, persistence, or sandbox authority.

## Control principle

The current Codex runtime tends to interpret many signals as direct permission to continue.

`muldex` changes that rule:

- signals become continuation candidates
- only the orchestration layer can approve continuation

## Non-goals for phase 1

- do not replace the Rust kernel with Python
- do not rewrite all tool implementations
- do not make Agently the owner of shell, sandbox, or local file authority
- do not change every upstream loop at once

## UI and operator compatibility

`muldex` is allowed to replace or augment orchestration internals, but it should preserve Codex operator familiarity by default.

Compatibility target:

- similar interaction rhythm
- similar terminal-first workflow
- similar session and thread mental model
- similar command and approval ergonomics
- no gratuitous changes to visible interface patterns while the runtime is still being stabilized

This means the first architecture phases optimize for runtime control and continuation governance, not for inventing a new front-end experience.

## Audio, video, and hyperframe context capability

`muldex` should support bringing audio and video into working context without abandoning Codex-style operator workflow.
It should also support `hyperframes` as first-class multimodal context units.

This does not mean injecting raw media into model context.

The intended pipeline is:

1. accept a local or referenced audio/video asset
2. derive bounded artifacts and hyperframes from it
3. expose those artifacts and hyperframes to orchestration and model layers as references plus compact summaries

Candidate derived artifacts:

- media metadata
- transcript
- subtitle track
- sampled keyframes for video
- segment or shot boundaries
- per-segment summaries
- hyperframes that align transcript, imagery, timestamps, and evidence
- compact evidence summary for operator and model use

The orchestration layer should reason over derived artifacts, hyperframes, and references, not opaque raw media payloads.

## Hyperframes

A hyperframe is a time-aligned multimodal context unit.

It can contain:

- one or more timestamps or a time range
- transcript text
- subtitle text
- visual summary or keyframe reference
- operator annotations
- evidence summary suitable for model context

Hyperframes are the preferred bridge between long-form media and bounded model-visible context.

## Multimodal model selection

Because `muldex` will support text, audio, image, video-derived context, and hyperframes, it needs explicit model-selection capability.

Selection must consider:

- input modality requirements
- output contract requirements
- latency tolerance
- cost budget
- context-window needs
- tool-use or orchestration behavior

This means model routing is a first-class runtime concern, not just a static config field.

## Reasoning harness and prohibitions

When `muldex` asks a model to reason, it should do so through an explicit harness layer rather than ad hoc prompt text alone.

The harness should define:

- what the model is trying to achieve
- what information it may rely on
- what actions it may propose versus directly perform
- what it must not do
- when it must stop, checkpoint, handoff, or self-correct

The list of prohibited behaviors should be explicit and reviewable.

Examples of prohibitions that should be representable:

- do not invent progress when no state changed
- do not continue after repeated no-progress iterations without escalation
- do not inject duplicate context
- do not claim a tool or backend succeeded without artifact or result evidence
- do not widen scope across repos or capabilities without justification
- do not silently switch execution mode or model class

This harness should be influenced by lessons from Codex and Claude Code prompt design, but it must become a kernel-governed contract, not a hidden prompt blob.

## MCP, skills, and Agent Data Protocol

`muldex` should treat MCP, skills, and Agent Data Protocol as first-class capability layers.

That means:

- orchestration can reason about available MCP tools and services
- orchestration can reason about installed or mounted skills
- orchestration can consume and emit structured agent data through an explicit protocol boundary

These should not remain implicit side channels. They must be representable in Rust-side capability descriptors and routing requests.

## Agent modes, subagents, and surface mobility

`muldex` should reserve first-class support for:

- agent modes such as read-only planning versus full-access build work
- general-purpose subagents for decomposition and search
- multiple operator surfaces such as terminal, desktop, and remote or detached sessions

These are not just UI concerns. They affect permissions, continuation policy, wakeup behavior, and routing decisions.

## Codex session continuity

`muldex` should eventually be able to consume enough Codex runtime/session state to continue useful programming work from an existing Codex conversation.

This does not require replaying the entire raw session transcript. The preferred path is:

1. export a bounded Codex runtime snapshot
2. translate it into `muldex` harness state
3. continue programming from structured state rather than raw transcript replay

The goal is practical continuity, not perfect byte-for-byte replay fidelity.

## Sandbox and approval compatibility

`muldex` should preserve Codex-like safety semantics by default:

- similar sandbox modes
- similar approval intent and escalation boundaries
- similar operator-facing permission review rhythm

New orchestration layers may add more structure, but they should not silently weaken Codex's existing trust and approval model.
