# Workstreams

## Experiment track

Small, independent prototypes that do not require editing upstream Codex first.

### A. Injection dedupe

- fingerprint hook-injected contexts
- suppress repeated injection when no new raw source context was added
- prefer short deltas over full repeated guidance blocks

### B. Trigger-turn guard

- classify mailbox wakeups
- allow immediate wake only for materially new work
- downgrade repeated follow-up chatter to queue-only

### C. Compaction progress guard

- track compaction frequency per thread/window
- detect repeated compact-without-state-change cycles
- emit a handoff or force a stop condition earlier

### D. Continue-reason instrumentation

- record why a turn continued:
  - model requested follow-up
  - tool future completed
  - pending input remained
  - trigger-turn wakeup

## Fork track

Prepare changes that can eventually be applied to an `openai/codex` fork.

### Candidate patch areas

- `codex-rs/core/src/hook_runtime.rs`
- `codex-rs/hooks/src/events/user_prompt_submit.rs`
- `codex-rs/core/src/session/turn.rs`
- `codex-rs/core/src/tasks/regular.rs`
- `codex-rs/core/src/tasks/mod.rs`
- `codex-rs/core/src/tools/handlers/multi_agents_v2/message_tool.rs`
