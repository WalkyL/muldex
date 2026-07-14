# Responses API Migration TODO

Reference: `docs/responses-api-migration-plan.md`

## Phase 1 — Request/response types

### Tasks
- [ ] Add Responses API request struct with model/instructions/input/tools/stream/include
- [ ] Add Responses API streaming event type enum
- [ ] Add non-streaming response struct
- [ ] Add tests for request serialization

### Acceptance
- [ ] Request serializes to correct JSON shape
- [ ] Event types cover all required SSE events

## Phase 2 — SSE streaming parser

### Tasks
- [ ] Parse SSE event: + data: lines
- [ ] Map each event type to ProviderStreamEvent
- [ ] Handle tool call deltas in Responses format
- [ ] Tests with mock SSE chunks

### Acceptance
- [ ] SSE parser tests pass
- [ ] All event types mapped correctly

## Phase 3 — ResponsesProvider client

### Tasks
- [ ] Implement InteractiveProvider for ResponsesProvider
- [ ] POST to {base_url}/responses with Accept: text/event-stream
- [ ] Stream parsing loop
- [ ] Fallback to non-streaming on error
- [ ] Tests with mock server

### Acceptance
- [ ] Mock server tests prove streaming works
- [ ] Non-streaming fallback works

## Phase 4 — Wire into interactive shell

### Tasks
- [ ] Replace OpenAiCompatibleProvider with ResponsesProvider in main.rs
- [ ] Set stream: true in interactive_turn.rs
- [ ] Verify with real llm-router

### Acceptance
- [ ] Streaming assistant text visible in transcript
- [ ] Tool calls processed correctly
- [ ] `cargo test -p muldex-runtime` passes
- [ ] `cargo test -p muldex-cli` passes
- [ ] `cargo build --release --target x86_64-pc-windows-msvc` passes