# Codex parity + ReAct gap plan

## Why this exists

Two confirmed gaps:

1. **TUI/UI behavior is still not Codex-CLI parity**
   - current muldex shell is a simplified ratatui shell
   - Codex CLI has a much richer composer, popup routing, terminal mode handling, transcript/event model, footer hints, and task-running interaction model

2. **Interactive prompts do not go to any LLM/provider today**
   - `handle_interactive_prompt()` only updates local runtime state and calls `driver.advance(...)`
   - no provider request is built
   - no HTTP request is sent to llm-router / openai-compatible endpoint
   - no streaming assistant deltas exist
   - no ReAct tool loop exists

## Current gap audit

### Current muldex prompt path

Current code path in `crates/muldex-cli/src/main.rs`:

1. user submits prompt
2. prompt stored into session transcript/history
3. `driver.advance(ContinueDecision { ... })`
4. deterministic runtime report rationale appended as assistant text

What is missing:

- provider config resolution into actual model call
- request building for OpenAI-compatible chat/completions
- assistant streaming into transcript
- tool call detection / execution / tool result turn continuation
- approval-aware tool gating
- busy/idle turn lifecycle in UI

### Current TUI gap vs Codex CLI

Codex CLI characteristics we need to converge on:

- richer terminal init/restore behavior
- composer-first state machine with popup routing
- history recall + reverse search integrated into composer
- transcript with committed cells + active live cell
- busy/streaming turn state reflected in footer/status
- better slash UX and popup lifecycle
- clearer separation between render model and execution events
- event-driven updates while agent turn is running

## Architecture freeze

Low-tier coding must follow this architecture exactly.

## Architecture

### A. UI parity lane

Keep UI work inside `crates/muldex-cli/src/interactive_tui/`.

Add/reshape modules toward these responsibilities:

1. `composer.rs`
   - Codex-like composer state machine
   - prompt buffer editing
   - history recall
   - reverse search state
   - slash popup state
   - submit/newline semantics

2. `transcript.rs`
   - transcript cell model
   - committed cells + active live cell
   - event-to-cell projection

3. `footer.rs`
   - footer hints
   - task-running status
   - approval/busy status badges

4. `terminal.rs`
   - raw mode
   - alternate screen
   - bracketed paste
   - focus change
   - keyboard enhancement where supported

5. `app.rs`
   - TUI loop local state
   - polls keyboard + runtime event channel
   - applies state transitions

### B. Execution lane

Execution work must not live inside TUI rendering.

Add new execution path split across core/runtime:

1. `crates/muldex-core/src/provider.rs`
   - provider config resolution
   - request/response structs for openai-compatible turns
   - provider trait boundary

2. `crates/muldex-runtime/src/interactive_turn.rs`
   - interactive turn orchestrator
   - prompt -> provider request -> event stream
   - state machine for idle / running / awaiting approval / failed / completed

3. `crates/muldex-runtime/src/react_loop.rs`
   - minimal ReAct loop
   - assistant message chunk handling
   - tool call capture
   - tool result injection
   - repeated until final assistant output or approval gate

4. `crates/muldex-runtime/src/ui_events.rs`
   - UI-safe event protocol consumed by TUI
   - examples:
     - `TurnStarted`
     - `AssistantDelta`
     - `AssistantMessageFinalized`
     - `ToolCallProposed`
     - `ApprovalRequested`
     - `ToolExecutionStarted`
     - `ToolExecutionFinished`
     - `TurnFailed`
     - `TurnCompleted`

### C. Provider implementation boundary

Provider implementation for this slice:

- first-class target: **OpenAI-compatible llm-router**
- config source: existing muldex config/provider resolution
- transport: `reqwest`
- support streaming SSE/chunked output for assistant text

### D. Tool loop scope

For this phase, tool loop is intentionally narrow:

- support no-tool assistant completion first
- then support **one local tool family** sufficient to prove ReAct plumbing end-to-end
- approval hook must exist before any mutating/local-shell tool execution

Recommended first tool family:

- `session.status`
- `session.list`
- `runtime.inspect`

These are read-only, easier than shell execution, and still prove assistant->tool->assistant continuation.

### E. Forbidden changes during coding

- no redesign of core runtime protocol beyond adding required event/request structs
- no daemon transport redesign
- no broad rewrite of all CLI commands
- no direct HTTP logic inside render modules
- no tool execution bypassing approval integration contract

## TDD slices

### Slice 1 — Gap-proof docs + provider resolution foundation

Implementation items:

- add provider request/response types
- add config resolution helpers from current provider config into resolved provider
- add tests for llm-router/default provider fallback resolution

Acceptance:

- provider resolution tests pass
- no HTTP calls yet

### Slice 2 — Interactive turn event model

Implementation items:

- define UI event enum
- define interactive turn lifecycle state
- define transcript cell projection inputs

Acceptance:

- tests prove idle/running/completed/failed transitions
- TUI can consume event objects without provider coupling

### Slice 3 — OpenAI-compatible provider client

Implementation items:

- add `reqwest`/runtime deps
- implement chat/completions request builder
- implement streaming parser for assistant text chunks
- add mock server tests

Acceptance:

- tests prove request payload includes model/messages
- tests prove streamed deltas become `AssistantDelta` events

### Slice 4 — Non-tool prompt execution path

Implementation items:

- replace local-only `driver.advance(...)` prompt path for interactive shell turns
- submit prompt to provider in background turn runner
- stream assistant text into transcript live cell
- finalize assistant message on completion

Acceptance:

- entering prompt causes actual provider call
- transcript shows streaming/final assistant output
- no-tool prompt round-trip tested with mock provider

### Slice 5 — Codex-style composer parity uplift

Implementation items:

- move more editing/search/slash logic into composer module
- align Enter/Ctrl+J/Ctrl+R/Esc/Tab behavior to Codex target semantics
- add footer/task-running state

Acceptance:

- PTY tests cover slash popup, reverse search, busy state
- interaction no longer feels like simple shell prompt

### Slice 6 — Minimal ReAct read-only tool loop

Implementation items:

- parse assistant tool calls
- execute read-only local tools through runtime layer
- append tool events/results to transcript
- continue follow-up model turn with tool result context

Acceptance:

- mock provider test proves assistant->tool->assistant loop
- approval hook path exists for future mutating tools

### Slice 7 — Human demo polish

Implementation items:

- align labels and footer semantics with Codex
- improve transcript cell styling for user/assistant/system/tool/approval
- ensure resumed sessions preserve transcript and composer history coherently

Acceptance:

- Windows x64 binary demoable by human
- prompt really reaches provider
- streaming visible
- at least one read-only ReAct tool path works end-to-end

## Final acceptance checklist

All required before closing todo:

1. codex parity gap doc and todo committed in repo workspace
2. `cargo test -p muldex-cli`
3. `cargo test -p muldex-runtime`
4. provider client tests with mock transport pass
5. PTY TUI tests pass
6. `cargo build --release --target x86_64-pc-windows-msvc`
7. real prompt triggers real provider HTTP request
8. assistant output streams into transcript
9. one ReAct read-only tool loop completes end-to-end
10. reviewer sign-off says demo scope acceptable

## Notes on delegated execution

Low-tier coding agent must:

- stay inside architecture above
- implement slices in order
- write tests before behavior changes where feasible
- report file list + tests + blockers at end

Validation agent must:

- check parity goals and LLM-call reality, not just compile/tests
- specifically verify prompt path is no longer local-only
