# Current Baseline

## Purpose

Freeze the current `muldex` baseline so future work can distinguish between:

- what is already implemented
- what is only planned
- what must remain stable while the runtime grows

This document is the operational baseline, not the long-range vision.

## Baseline summary

`muldex` is currently a Rust workspace with three active layers:

- `muldex-core`
- `muldex-runtime`
- `muldex-cli`

The project is no longer only a protocol sketch. It now has a minimal runtime step loop:

1. build or ingest a structured request
2. run the reasoning harness
3. convert the decision into runtime step input
4. advance runtime state one bounded cycle
5. emit a structured run report

## What is implemented now

### 1. `muldex-core`

Location:

- `crates/muldex-core/src/protocol.rs`
- `crates/muldex-core/src/reasoning_harness.rs`
- `crates/muldex-core/src/upstream_adapter.rs`
- `crates/muldex-core/src/agently.rs`
- `crates/muldex-core/src/policy.rs`

Current responsibilities:

- continuation and orchestration protocol types
- multimodal and capability registry protocol types
- approval and interrupt protocol state
- run and cycle report protocol state
- deterministic reasoning harness policy
- Codex snapshot adapters
- narrow Agently sidecar contract

Important implemented protocol areas:

- `ContinueRequest`
- `ContinueDecision`
- `PermissionRequest` and `PendingApprovalState`
- `PendingInterrupt` and `InterruptQueueState`
- `RunReport` and `CycleSummary`
- `ExecutionMode` and runtime-mode state

### 2. `muldex-runtime`

Location:

- `crates/muldex-runtime/src/runtime.rs`

Current responsibilities:

- hold runtime state around one active request
- apply one bounded runtime transition from a structured decision
- ingest external runtime events such as interrupts and approval decisions
- resume bounded runtime progress after an approval-unblock event
- consume safe-point interrupts when requested
- persist approval-blocked state into runtime-owned request state
- emit cycle-scoped `RunReport`
- host multiple session-scoped runtime drivers behind a simple command router
- export and restore in-memory session and host snapshots through explicit data structures
- own a daemon-shell lifecycle object around host state and snapshot path management
- provide an explicit continuity layer for native resume/export and external snapshot import
- provide local daemon ownership metadata and single-owner lock primitives
- provide partial harness-safe report-layer compression helpers
- provide local file-based daemon command transport skeleton
- provide lease-style heartbeat fields and stale-status classification helpers
- provide stable client-facing daemon and session view schemas
- provide versioned client contract metadata and read-only capability allowlist
- document concrete `client-view-v1` JSON examples for external clients
- enforce minimal read-only client command gating at CLI boundary
- provide versioned daemon command and response envelope tags for client/server transport

Important implemented runtime objects:

- `RuntimeState`
- `RuntimeDriver`
- `RuntimeHost`
- `RuntimeSessionSnapshot`
- `RuntimeHostSnapshot`
- `RuntimeDaemon`
- `continuity` module APIs for resume, import, and export
- `daemon_local` module for local lock and daemon state metadata
- `daemon_transport` module for local file-based command and response envelopes
- heartbeat refresh and stale-status helpers in local daemon ownership layer
- `compression` module for exact-only report-layer dedup helpers
- `client_views` module for stable daemon and session inspection schemas
- `ClientContractInfo` and read-only client capability descriptors
- `RuntimeCommand`
- `RuntimeCommandResult`
- `RuntimePhase`
- `RuntimeEvent`
- `RuntimeStepInput`
- `RuntimeStepResult`
- `advance_runtime(...)`
- `ingest_runtime_event(...)`
- `drive_runtime(...)`
- `drive_runtime_script(...)`
- `resume_runtime_after_event(...)`

This is a minimal runtime kernel, not yet a daemon or scheduler.

The preferred runtime entrypoint is now the driver object plus the normalized runtime command layer, with the lower-level functions retained as building blocks.

For daemon-facing integration, the current outer shell is the runtime host layer, which manages session-scoped drivers in memory.

The current persistence boundary is data-only: host and session snapshots can be exported and restored in memory, but file persistence and background process ownership are not implemented yet.

The current persistence boundary also includes file-backed host snapshot save and load through the runtime host API. This is sufficient for basic save, restore, and resume demonstrations, but not yet a full daemon persistence model.

The current continuity boundary explicitly separates:

- native host resume from `RuntimeHostSnapshot`
- native host and session export
- external snapshot import into `RuntimeState`
- raw latest-report export and compressed latest-report export
- optional session export views with raw or compressed report payloads

Current external import coverage includes Codex bootstrap, live, and signal snapshots through the runtime continuity module.

The current daemon boundary is a shell object that owns:

- lifecycle status
- host ownership
- snapshot path selection
- boot, save, load, and shutdown behavior

It does not yet provide IPC transport or background service orchestration.

The current local ownership boundary includes:

- file-backed daemon lock metadata
- file-backed daemon state metadata
- single-owner acquisition and release primitives
- `RuntimeDaemon` integration with lock and state metadata on boot, save, and shutdown
- heartbeat refresh and stale-status classification

It does not yet include stale-owner takeover policy.

The current local transport boundary includes:

- file-based command envelopes
- file-based response envelopes
- command and response directory layout
- one-shot daemon-side command processing through transport skeleton
- processed commands archived after handling while responses remain available

The current active daemon loop boundary includes:

- single processing cycle execution
- bounded foreground loop execution over local transport

It does not yet include active daemon polling, blocking waits, or socket or pipe transport.

The current compression boundary is intentionally narrow:

- exact-only dedup at report layer
- cycle-summary stubbing with preserved identity fields
- no dedup across harness core or live safety and approval state
- latest-report export can now expose raw or compressed report views through continuity helpers
- session export views can choose raw or compressed latest-report payloads without changing raw snapshot schema

### 3. `muldex-cli`

Location:

- `crates/muldex-cli/src/main.rs`

Current responsibilities:

- produce sample harness requests
- ingest request JSON and Codex snapshot JSON
- run the reasoning harness
- advance the runtime one bounded step
- print both decision output and runtime-step summary
- provide a default interactive shell entrypoint when invoked without a subcommand
- preserve a Codex-style operator shell direction through slash commands, persisted shell sessions, prompt history, and stable redraw behavior in TTY mode

Supported command surfaces:

- default interactive shell entry via `muldex` with optional prompt
- `decide-sample`
- `decide-file`
- `decide-codex-snapshot`
- `decide-workspace`
- `demo-approval-resume`
- `demo-host-persistence`
- `save-host-snapshot`
- `load-host-snapshot`
- `import-codex-snapshot`
- `export-session-view`
- `daemon-boot-empty`
- `daemon-boot-load`
- `daemon-save`
- `daemon-status`
- `daemon-send-command`
- `daemon-read-response`
- `daemon-serve-once`
- `daemon-serve-loop`
- `client-status`
- `client-send-command`
- `client-read-response`
- `client-list-sessions`
- `client-inspect-session`
- `client-export-session`

Current interactive shell compatibility slice includes:

- persisted shell sessions with `/new`, `/sessions`, and `/resume [id]`
- runtime-backed `/model`, `/approval`, and `/compact`
- slash command hints, keyboard selection, and apply flows
- prompt history recall and reverse history search
- multi-line prompt composition through explicit newline insertion

## Implemented behavioral baseline

The following behaviors should now be treated as part of the baseline.

### Approval-aware continuation

The reasoning harness distinguishes between:

- blocked on approval, but other work may continue
- blocked on approval, and no other work may continue

Current behavior:

- queue-only when approval blocks one path but not all useful work
- handoff when approval blocks the run entirely

### Interrupt-aware continuation

The reasoning harness recognizes queued interrupts that should be absorbed at a safe point.

Current behavior:

- immediate safe-point interrupts bias the decision toward same-turn continuation
- the runtime can consume those interrupts during `advance_runtime(...)`

### Report-producing runtime step

Runtime state transitions now produce a structured report rather than only a boolean or mode.

Current behavior:

- `RunOutcome::InProgress`
- `RunOutcome::WaitingForApproval`
- `RunOutcome::Checkpointed`
- `RunOutcome::HandedOff`
- `RunOutcome::Stopped`

### Approval-unblock resume path

The runtime now has an explicit resume path after approval state changes.

Current behavior:

- runtime can enter `WaitingForApproval`
- runtime can ingest an approval decision event
- approved requests can return to `Ready`
- bounded runtime driving can resume from that recovered state

### Scripted interleaved runtime driving

The runtime now supports a simple scripted driver that can interleave:

- external events
- continuation decisions

Current behavior:

- event and decision steps can be driven through one bounded script
- waiting-for-approval blocks decision execution until an event clears the boundary
- terminal phases stop the script early

## What is intentionally not implemented yet

The following are still outside the current baseline:

- daemon or persistent background runtime process
- reconnectable clients
- real event loop or scheduler
- runtime persistence across process restarts with daemon-managed lifecycle and recovery policy
- real approval queue storage
- IPC transport and true background process orchestration
- active daemon command-processing loop over local transport skeleton
- richer replay and export surfaces beyond current host/session and runtime-state boundaries
- stale-owner recovery and lockfile lease policy
- forced takeover and lease-based recovery policy
- full retention-class propagation across all runtime context layers
- real interrupt source integration
- live feedback injection into the upstream Codex runtime
- multi-agent runtime ownership and coordination

These are planned directions, not current guarantees.

## Verification baseline

Current expected verification command:

```powershell
cargo test
```

At the time this baseline was written, the workspace passes tests across:

- `muldex-core`
- `muldex-runtime`
- `muldex-cli`

The important currently covered behaviors include:

- protocol round-trip serialization
- Codex snapshot adapter mapping
- harness no-progress escalation
- harness self-correction entry
- approval-blocked harness branching
- safe-point interrupt harness branching
- runtime interrupt consumption
- runtime approval wait outcome
- runtime approval-unblock resume path
- scripted interleaved event and decision driving
- runtime checkpointed reporting
- multi-session in-memory host routing
- in-memory snapshot export and restore for host and session state
- file-backed host snapshot save and load
- daemon-shell lifecycle around host ownership and snapshot management
- explicit resume, import, and export continuity APIs
- local daemon lock and state metadata shell
- exact-only report-layer compression and unchanged-summary stubs
- raw and compressed latest-report export views
- optional raw/compressed session export views

## Stable baseline constraints

Until intentionally revised, future work should preserve these constraints:

- Rust remains the owner of orchestration and runtime policy
- Agently remains a sidecar seam, not the kernel owner
- approval and interrupt semantics stay explicit in protocol, not hidden in prompts
- runtime decisions must remain inspectable through structured outputs
- multimodal support continues to use bounded derived artifacts rather than raw media injection

## Recommended next step after this baseline

The next implementation step should build on this baseline rather than bypass it.

Preferred near-term direction:

- grow `muldex-runtime` from a single-step kernel into a small state machine with explicit event ingestion and repeated cycle advancement

This keeps the current architecture coherent:

- `muldex-core` defines and interprets policy
- `muldex-runtime` owns state transitions
- `muldex-cli` remains an operator and debugging surface
