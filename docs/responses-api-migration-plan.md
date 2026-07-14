# Responses API Migration Plan

## Why

llm-router returns 403 for `stream: true` on Chat Completions API.
Codex CLI uses Responses API (`POST /responses`) and streaming works fine.
Switch muldex to Responses API for streaming compatibility.

## Architecture

### New module: `crates/muldex-runtime/src/responses_provider.rs`
- Responses API request builder
- SSE parser for Responses API event types
- Implements `InteractiveProvider` trait

### Changes to existing code

1. `crates/muldex-core/src/provider.rs` — no changes needed (trait boundary same)
2. `crates/muldex-runtime/src/interactive_turn.rs` — change `stream: false` back to `stream: true`, constructor uses `ResponsesProvider`
3. `crates/muldex-cli/src/main.rs` — change `OpenAiCompatibleProvider::default()` to `ResponsesProvider::default()`

### Responses API request format

```
POST {base_url}/responses
Content-Type: application/json
Accept: text/event-stream

{
  "model": "gpt-5.4",
  "instructions": "...",
  "input": [
    {
      "type": "message",
      "role": "user",
      "content": [{"type": "input_text", "text": "hello"}]
    }
  ],
  "tools": [...],
  "tool_choice": "auto",
  "stream": true,
  "include": ["reasoning.encrypted_content"]
}
```

### Responses API streaming events

| Event | Meaning |
|-------|---------|
| `response.created` | Response started |
| `response.output_item.added` | New output item added |
| `response.content_part.added` | Content part added to item |
| `response.output_text.delta` | Assistant text delta |
| `response.output_text.done` | Text segment complete |
| `response.function_call_arguments.delta` | Tool call args delta |
| `response.function_call_arguments.done` | Tool call args complete |
| `response.completed` | Response fully done |
| `error` | Error |

### SSE line format
```
event: response.output_text.delta
data: {"type":"response.output_text.delta","delta":"Hello","item_id":"item_1","output_index":0,"content_index":0}
```

## TDD Slices

### Slice 1 — Responses API request/response types
- Define request struct
- Define streaming event types
- Define non-streaming response struct
- Tests for serialization

### Slice 2 — SSE streaming parser
- Parse `event:` + `data:` SSE lines
- Parse each event type into ProviderStreamEvent
- Tests with mock SSE chunks

### Slice 3 — ResponsesProvider client
- Implements InteractiveProvider
- POST to /responses with proper headers
- Stream parsing loop
- Fallback to non-streaming on error
- Tests with mock server

### Slice 4 — Wire into main.rs
- Replace OpenAiCompatibleProvider with ResponsesProvider
- Set stream: true in interactive_turn
- Test with real llm-router

## Forbidden changes
- No redesign of InteractiveProvider trait
- No changes to daemon/transport schema
- No changes to TUI rendering code
- No changes to session persistence