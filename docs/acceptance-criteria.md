# Acceptance Criteria

## Documentation acceptance

- architecture boundary is explicit
- ownership between Rust kernel and external runtime is explicit
- continuation decision contract is documented
- phase 1 work items each have deliverables and acceptance checks
- UI/UX compatibility goal with Codex is explicitly documented
- audio/video-to-context capability is described as a bounded artifact pipeline, not as raw upload magic
- capability audit exists for model/runtime surfaces that could force kernel redesign later

### Reasoning harness

- protocol can express explicit reasoning constraints and prohibitions
- prohibited behaviors are reviewable outside a single hidden system prompt
- long-running and self-correcting tasks have explicit stop/escalation semantics

## Code acceptance for phase 1

- workspace builds with `cargo check`
- orchestration types serialize cleanly to JSON
- traits compile without Python present
- at least one deterministic local orchestrator implementation exists
- no phase 1 artifact requires abandoning Codex-style operator workflow assumptions

### Audio/video context

- audio/video support preserves Codex-style operator workflow
- raw media is not injected directly into model context
- derived artifacts are bounded, referenceable, and inspectable

### Capability coverage

- protocol can describe reasoning controls, structured-output controls, execution modes, and context features
- MCP, skills, and Agent Data Protocol can be described through explicit capability snapshots
- future media and document support can be added without redesigning orchestration ownership

### Long-running execution

- protocol can represent progress, checkpoint, recovery, and self-correction signals
- long tasks can continue after recoverable failure without relying on blind repetition
- anti-spin semantics are explicit enough to stop low-value continuation

## Behavioral acceptance for later runtime guards

### Injection dedupe

- repeated identical injected context does not get re-applied blindly
- empty or whitespace-only injected context is ignored
- policy outcome is visible in logs, traces, or explicit decisions

### Trigger-turn control

- wakeup can be downgraded when no meaningful state change exists
- queue-only behavior remains available

### Compaction progress

- repeated compact-without-progress sequences are detectable
- system can recommend stop or handoff instead of blind continuation

## Review flow acceptance

### Coding pass

- implementation slice is completed with focused scope
- code compiles or any blocker is explicitly recorded
- a human-runnable real-environment invocation path exists when the slice claims runtime behavior value

### Validation pass

- types and trait boundaries are reviewed for missing fields or bad ownership
- acceptance criteria are checked against actual artifacts
- gaps are recorded as follow-up items instead of implicit assumptions

### Real environment validation

- operators can run the binary against a real workspace path
- output is understandable without reading source code
- simulated anti-spin and recovery scenarios are available through stable CLI flags or files
