# Core Data Structures

## Design intent

These structures define the protocol between the Rust kernel and the orchestration layer.

They must make continuation observable, testable, and replayable.

## ContinueReason

Represents the direct reason a turn is asking to continue.

Candidate variants:

- `ModelFollowUp`
- `ToolResult`
- `PendingInput`
- `TriggerTurnWakeup`
- `ManualUserRequest`
- `CompactionRecovery`

## StateChangeKind

Represents whether a meaningful state transition happened since the last continuation decision.

Candidate variants:

- `CodeEdit`
- `NewConfirmedFinding`
- `NewError`
- `UserDecision`
- `ToolSideEffect`
- `NoMeaningfulChange`

## ContextPressure

Represents current context state at decision time.

Suggested fields:

- `model_context_window: Option<u32>`
- `active_context_tokens: Option<u32>`
- `auto_compact_scope_tokens: Option<u32>`
- `auto_compact_limit: Option<u32>`
- `tokens_until_compaction: Option<u32>`
- `recent_compaction_count: u32`
- `last_compaction_had_state_change: bool`

## ContinueRequest

The main input to orchestration.

Suggested fields:

- `thread_id: String`
- `turn_id: String`
- `objective: String`
- `constraints: Vec<String>`
- `continue_reason: ContinueReason`
- `recent_state_changes: Vec<StateChangeKind>`
- `working_hypothesis: Option<String>`
- `last_agent_message: Option<String>`
- `pending_input_count: usize`
- `trigger_turn_pending: bool`
- `tool_call_count_this_turn: usize`
- `context_pressure: ContextPressure`
- `duplicate_injection_detected: bool`
- `repeated_follow_up_count: u32`

## ContinueMode

How approved continuation should happen.

Candidate variants:

- `SameTurn`
- `NextTurn`
- `QueueOnly`
- `Handoff`
- `Stop`

## ContinueDecision

Output of orchestration.

Suggested fields:

- `allow_continue: bool`
- `mode: ContinueMode`
- `rationale: String`
- `next_action: Option<String>`
- `suppress_duplicate_injection: bool`
- `downgrade_trigger_turn: bool`
- `request_compaction: bool`
- `request_handoff_summary: bool`

## PlannerRequest

Optional sidecar request used when an external runtime helps decide next action.

Suggested fields:

- `objective: String`
- `constraints: Vec<String>`
- `confirmed_facts: Vec<String>`
- `open_questions: Vec<String>`
- `continue_reason: ContinueReason`
- `context_pressure: ContextPressure`

## PlannerResponse

Suggested fields:

- `recommended_mode: ContinueMode`
- `recommended_next_action: Option<String>`
- `rationale: String`
- `confidence: f32`

## Audio and video context types

These types are needed if `muldex` adds Kimi-style media-to-context workflows.

### MediaAssetRef

Suggested fields:

- `asset_id: String`
- `kind: MediaKind`
- `source: MediaSource`
- `display_name: Option<String>`

### MediaKind

Candidate variants:

- `Image`
- `Audio`
- `Video`

### MediaSource

Candidate variants:

- `LocalPath { path: String }`
- `RemoteUrl { url: String }`
- `ManagedArtifact { artifact_id: String }`

### TimeRangeMs

Suggested fields:

- `start_ms: u64`
- `end_ms: u64`

### DerivedMediaArtifact

Suggested fields:

- `artifact_id: String`
- `media_asset_id: String`
- `kind: DerivedMediaArtifactKind`
- `time_range: Option<TimeRangeMs>`
- `summary: Option<String>`
- `reference: Option<String>`

### DerivedMediaArtifactKind

Candidate variants:

- `Metadata`
- `Transcript`
- `SubtitleTrack`
- `Keyframe`
- `ShotSummary`
- `SegmentSummary`
- `EvidenceSummary`

### MediaContextEnvelope

Suggested fields:

- `asset: MediaAssetRef`
- `derived_artifacts: Vec<DerivedMediaArtifact>`
- `operator_summary: String`
- `model_summary: String`
- `token_budget_hint: Option<u32>`

## Multimodal model selection types

### InputModality

Candidate variants:

- `Text`
- `Image`
- `Audio`
- `VideoDerived`
- `Hyperframe`

### OutputModality

Candidate variants:

- `Text`
- `StructuredJson`
- `ToolCall`
- `ReasoningSummary`

### ModelCapabilityDescriptor

Suggested fields:

- `model_id: String`
- `provider_id: String`
- `input_modalities: Vec<InputModality>`
- `output_modalities: Vec<OutputModality>`
- `max_context_window: Option<u32>`
- `supports_tool_use: bool`
- `supports_parallel_tool_use: bool`
- `supports_long_context: bool`
- `supports_streaming: bool`
- `supports_audio_input: bool`
- `supports_image_input: bool`
- `supports_video_context: bool`
- `supports_hyperframes: bool`

### ModelSelectionIntent

Suggested fields:

- `required_input_modalities: Vec<InputModality>`
- `required_output_modalities: Vec<OutputModality>`
- `prefer_long_context: bool`
- `prefer_low_latency: bool`
- `prefer_low_cost: bool`
- `requires_tool_use: bool`
- `task_label: Option<String>`

### ModelSelectionRequest

Suggested fields:

- `thread_id: String`
- `turn_id: String`
- `intent: ModelSelectionIntent`
- `context_pressure: ContextPressure`
- `media_assets: Vec<MediaAssetRef>`

### ModelSelectionDecision

Suggested fields:

- `selected_model_id: String`
- `selected_provider_id: String`
- `rationale: String`
- `fallback_model_id: Option<String>`

## Capability registry types

### McpCapabilityDescriptor

Suggested fields:

- `server_id: String`
- `tool_names: Vec<String>`
- `resource_names: Vec<String>`
- `prompt_names: Vec<String>`

### SkillCapabilityDescriptor

Suggested fields:

- `skill_id: String`
- `display_name: Option<String>`
- `capability_tags: Vec<String>`
- `provides_tools: Vec<String>`
- `provides_prompts: Vec<String>`

### AgentDataCapabilityDescriptor

Suggested fields:

- `protocol_id: String`
- `schema_ids: Vec<String>`
- `supports_ingest: bool`
- `supports_emit: bool`

### CapabilityRegistrySnapshot

Suggested fields:

- `mcp: Vec<McpCapabilityDescriptor>`
- `skills: Vec<SkillCapabilityDescriptor>`
- `agent_data_protocols: Vec<AgentDataCapabilityDescriptor>`

## Acceptance requirements for these structures

- serializable to JSON
- stable enough for logging and replay
- small enough to inspect in debugging output
- expressive enough to explain every continuation decision
- raw audio or video is never injected into model context directly
- all model-visible media context is bounded and referenceable
