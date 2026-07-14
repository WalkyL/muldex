# Codex parity + ReAct TODO

Reference: `docs/codex-parity-react-gap-plan.md`

## Phase 1 — Provider resolution foundation

### Tasks
- [ ] Add provider/runtime gap architecture doc references into working docs
- [ ] Add resolved provider config model in `muldex-core`
- [ ] Add fallback resolution rules for default provider / llm-router
- [ ] Add tests for configured/unconfigured provider resolution

### Acceptance
- [ ] Provider resolution tests pass
- [ ] No UI module makes direct config/network decisions

## Phase 2 — Interactive turn event model

### Tasks
- [ ] Add UI event enum for transcript + status updates
- [ ] Add interactive turn lifecycle model
- [ ] Add transcript live-cell projection model
- [ ] Add tests for lifecycle transitions

### Acceptance
- [ ] Runtime can express started/streaming/completed/failed without TUI coupling
- [ ] Tests prove state transitions

## Phase 3 — OpenAI-compatible provider client

### Tasks
- [ ] Add HTTP/runtime dependencies
- [ ] Build chat/completions request payloads from prompt + model + provider
- [ ] Implement streaming response parser
- [ ] Add mock transport/server tests

### Acceptance
- [ ] Mock tests prove real request payload emitted
- [ ] Streaming assistant deltas parsed into runtime events

## Phase 4 — Real prompt execution path

### Tasks
- [ ] Replace local-only interactive prompt path
- [ ] Submit prompt to provider turn runner
- [ ] Stream assistant text into transcript live cell
- [ ] Finalize assistant message into committed transcript cell
- [ ] Preserve history/session persistence

### Acceptance
- [ ] Prompt really reaches provider
- [ ] Assistant output no longer just `interactive prompt: ...`
- [ ] PTY/integration tests cover non-tool round trip

## Phase 5 — Codex-style composer parity uplift

### Tasks
- [ ] Extract composer state machine module
- [ ] Align Enter/Ctrl+J/Tab/Esc/Ctrl+R semantics
- [ ] Add footer/task-running status
- [ ] Improve slash popup lifecycle and transcript busy state

### Acceptance
- [ ] Interaction closer to Codex than shell prompt
- [ ] PTY tests cover popup/search/busy state

## Phase 6 — Minimal ReAct read-only tool loop

### Tasks
- [ ] Add tool call parsing from assistant output
- [ ] Add read-only runtime tools (`session.status` / `session.list` / `runtime.inspect`)
- [ ] Inject tool results into follow-up model turn
- [ ] Add approval hook scaffolding for future mutating tools

### Acceptance
- [ ] Assistant->tool->assistant test passes end-to-end
- [ ] Tool loop visible in transcript

## Phase 7 — Demo polish and final gate

### Tasks
- [ ] Align top bar/footer/transcript semantics with Codex target
- [ ] Improve status badges and live transcript styling
- [ ] Verify resume/new session flows still coherent
- [ ] Human-demo pass on Windows x64

### Acceptance
- [ ] `cargo test -p muldex-cli`
- [ ] `cargo test -p muldex-runtime`
- [ ] `cargo build --release --target x86_64-pc-windows-msvc`
- [ ] Real prompt causes real provider HTTP request
- [ ] Streaming visible in TUI
- [ ] One read-only ReAct loop works
- [ ] Reviewer sign-off complete
