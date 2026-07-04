# Problem Statement

## Primary issue

Codex CLI can lose focus and enter low-value spinning loops. The visible symptoms include:

- repeated checking without new edits
- repeated summarization or compaction without enough new state change
- long threads that survive compaction but keep extending themselves
- idle sessions waking up again without clear user intent

## Current hypotheses from upstream code reading

### 1. Hook injection loop

`UserPromptSubmit` and related hooks can emit `additional_contexts`, and those contexts are recorded into conversation history as model-visible items.

Observed properties:

- no built-in deduplication
- no built-in size cap at flatten time
- plain stdout can become injected context

Risk:

- a safety or budget hook may itself bloat the thread or repeatedly restate guidance

### 2. Tool follow-up loop

Tool calls set `needs_follow_up = true`, tool results are written back into history, and the model is allowed to continue from the result.

Risk:

- tool-heavy tasks can become very long single-turn loops

### 3. Pending-input loop

`RegularTask` reruns `run_turn()` while `has_pending_input()` remains true.

Risk:

- even after one turn "finishes", queued work can immediately continue the same task cycle

### 4. Trigger-turn wakeup loop

Mailbox messages with `trigger_turn = true` can wake an idle session and start a new turn automatically.

Risk:

- agent-to-agent follow-up can keep a session alive with weak stop conditions

### 5. Compaction-without-progress loop

Auto-compaction reduces context but does not by itself break follow-up. If the task still wants to continue, the thread may compact and continue repeatedly.

Risk:

- compaction becomes a cost-control loop, not a progress-control loop

## Design goals

1. Distinguish model-driven continuation from scheduler-driven continuation
2. Make context injection idempotent across nearby turns
3. Require stronger evidence before wakeups with `trigger_turn = true`
4. Detect repeated compaction without a meaningful state change
5. Preserve enough telemetry to explain why a turn continued
