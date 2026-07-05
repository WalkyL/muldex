use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContinueReason {
    ModelFollowUp,
    ToolResult,
    PendingInput,
    TriggerTurnWakeup,
    ManualUserRequest,
    CompactionRecovery,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StateChangeKind {
    CodeEdit,
    NewConfirmedFinding,
    NewError,
    UserDecision,
    ToolSideEffect,
    NoMeaningfulChange,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ContextPressure {
    pub model_context_window: Option<u32>,
    pub active_context_tokens: Option<u32>,
    pub auto_compact_scope_tokens: Option<u32>,
    pub auto_compact_limit: Option<u32>,
    pub tokens_until_compaction: Option<u32>,
    pub recent_compaction_count: u32,
    pub last_compaction_had_state_change: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgressSnapshot {
    pub completed_steps: u32,
    pub total_steps_hint: Option<u32>,
    pub last_meaningful_progress_at_ms: Option<u64>,
    pub no_progress_iteration_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryReason {
    ToolFailure,
    PartialResult,
    ContextPressure,
    TransientModelFailure,
    ExternalDependencyUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoverySnapshot {
    pub last_recovery_reason: Option<RecoveryReason>,
    pub recovery_attempt_count: u32,
    pub last_recovery_had_progress: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointRef {
    pub checkpoint_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub created_at_ms: u64,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelfCorrectionState {
    pub active: bool,
    pub correction_attempt_count: u32,
    pub last_correction_target: Option<String>,
    pub last_correction_had_progress: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PostCompactionState {
    pub pending_post_compaction: bool,
    pub first_post_compaction_turn: bool,
    pub compaction_window_id: Option<String>,
    pub last_compaction_checkpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillInvocationState {
    pub skill_id: String,
    pub invocation_ref: Option<String>,
    pub invoked_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RuntimeModeState {
    pub active_agent_mode: Option<String>,
    pub previous_agent_mode: Option<String>,
    pub mode_transition_pending_guidance: bool,
    pub invoked_skills: Vec<SkillInvocationState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Audio,
    Video,
    Hyperframe,
    Document,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MediaSource {
    LocalPath { path: String },
    RemoteUrl { url: String },
    ManagedArtifact { artifact_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaAssetRef {
    pub asset_id: String,
    pub kind: MediaKind,
    pub source: MediaSource,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeRangeMs {
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DerivedMediaArtifactKind {
    Metadata,
    Transcript,
    SubtitleTrack,
    Keyframe,
    ShotSummary,
    SegmentSummary,
    EvidenceSummary,
    Hyperframe,
    AlignmentMap,
    AsrWordTiming,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DerivedMediaArtifact {
    pub artifact_id: String,
    pub media_asset_id: String,
    pub kind: DerivedMediaArtifactKind,
    pub time_range: Option<TimeRangeMs>,
    pub summary: Option<String>,
    pub reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hyperframe {
    pub hyperframe_id: String,
    pub source_media_asset_id: String,
    pub time_range: TimeRangeMs,
    pub transcript: Option<String>,
    pub subtitle_text: Option<String>,
    pub visual_summary: Option<String>,
    pub keyframe_artifact_ids: Vec<String>,
    pub operator_annotations: Vec<String>,
    pub evidence_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaContextEnvelope {
    pub asset: MediaAssetRef,
    pub derived_artifacts: Vec<DerivedMediaArtifact>,
    pub hyperframes: Vec<Hyperframe>,
    pub operator_summary: String,
    pub model_summary: String,
    pub token_budget_hint: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InputModality {
    Text,
    Image,
    Audio,
    VideoDerived,
    Hyperframe,
    DocumentDerived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputModality {
    Text,
    StructuredJson,
    ToolCall,
    ReasoningSummary,
    CitationSet,
    Patch,
    Media,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReasoningMode {
    None,
    Low,
    Medium,
    High,
    SummaryOnly,
    RawIfAvailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StructuredOutputMode {
    Freeform,
    JsonSchema,
    RequiredKeys,
    ToolArguments,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionMode {
    Interactive,
    Streaming,
    Background,
    Resumable,
    Batch,
    Scheduled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionSurfaceKind {
    Terminal,
    Desktop,
    Remote,
    Detached,
    Web,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SandboxModeDescriptor {
    ReadOnly,
    WorkspaceWrite,
    FullAccess,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApprovalPolicyDescriptor {
    Never,
    Ask,
    OnRequest,
    UnlessTrusted,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionContextSnapshot {
    pub sandbox_mode: SandboxModeDescriptor,
    pub approval_policy: ApprovalPolicyDescriptor,
    pub permission_profile_summary: String,
    pub network_access_enabled: bool,
    pub requires_explicit_approval_for_next_step: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexSessionContinuationSnapshot {
    pub source_thread_id: String,
    pub source_turn_id: String,
    pub source_model: String,
    pub source_provider: String,
    pub active_agent_mode: Option<String>,
    pub safety: PermissionContextSnapshot,
    pub reference_context_present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSurfaceDescriptor {
    pub surface_id: String,
    pub kind: SessionSurfaceKind,
    pub supports_interactive_input: bool,
    pub supports_background_execution: bool,
    pub supports_resume: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentModeDescriptor {
    pub mode_id: String,
    pub display_name: String,
    pub read_only: bool,
    pub allows_file_edits: bool,
    pub allows_shell_execution: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubagentCapabilityDescriptor {
    pub subagent_id: String,
    pub role: String,
    pub capability_tags: Vec<String>,
    pub can_trigger_turn: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelCapabilityDescriptor {
    pub model_id: String,
    pub provider_id: String,
    pub input_modalities: Vec<InputModality>,
    pub output_modalities: Vec<OutputModality>,
    pub max_context_window: Option<u32>,
    pub supports_tool_use: bool,
    pub supports_parallel_tool_use: bool,
    pub supports_long_context: bool,
    pub supports_streaming: bool,
    pub supports_audio_input: bool,
    pub supports_image_input: bool,
    pub supports_video_context: bool,
    pub supports_hyperframes: bool,
    pub reasoning_modes: Vec<ReasoningMode>,
    pub structured_output_modes: Vec<StructuredOutputMode>,
    pub execution_modes: Vec<ExecutionMode>,
    pub supports_citations: bool,
    pub supports_prompt_caching: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelSelectionIntent {
    pub required_input_modalities: Vec<InputModality>,
    pub required_output_modalities: Vec<OutputModality>,
    pub prefer_long_context: bool,
    pub prefer_low_latency: bool,
    pub prefer_low_cost: bool,
    pub requires_tool_use: bool,
    pub task_label: Option<String>,
    pub reasoning_mode: Option<ReasoningMode>,
    pub structured_output_mode: Option<StructuredOutputMode>,
    pub execution_mode: Option<ExecutionMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelSelectionRequest {
    pub thread_id: String,
    pub turn_id: String,
    pub intent: ModelSelectionIntent,
    pub context_pressure: ContextPressure,
    pub media_assets: Vec<MediaAssetRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelSelectionDecision {
    pub selected_model_id: String,
    pub selected_provider_id: String,
    pub rationale: String,
    pub fallback_model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpCapabilityDescriptor {
    pub server_id: String,
    pub tool_names: Vec<String>,
    pub resource_names: Vec<String>,
    pub prompt_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillCapabilityDescriptor {
    pub skill_id: String,
    pub display_name: Option<String>,
    pub capability_tags: Vec<String>,
    pub provides_tools: Vec<String>,
    pub provides_prompts: Vec<String>,
    pub declared_input_modalities: Vec<InputModality>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentDataCapabilityDescriptor {
    pub protocol_id: String,
    pub schema_ids: Vec<String>,
    pub supports_ingest: bool,
    pub supports_emit: bool,
    pub supports_streaming_updates: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AsrBackendKind {
    Native,
    WhisperLike,
    ProviderManaged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AsrCapabilityDescriptor {
    pub backend_id: String,
    pub backend_kind: AsrBackendKind,
    pub supports_word_timestamps: bool,
    pub supports_speaker_diarization: bool,
    pub supports_multilingual: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlignmentBackendKind {
    TranscriptToAudio,
    TranscriptToVideo,
    CrossModalEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AlignmentCapabilityDescriptor {
    pub backend_id: String,
    pub backend_kind: AlignmentBackendKind,
    pub supports_frame_alignment: bool,
    pub supports_word_alignment: bool,
    pub supports_segment_alignment: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GenerativeBackendKind {
    DiffusionImage,
    DiffusionVideo,
    AudioGeneration,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerativeBackendCapabilityDescriptor {
    pub backend_id: String,
    pub backend_kind: GenerativeBackendKind,
    pub supports_image_generation: bool,
    pub supports_video_generation: bool,
    pub supports_audio_generation: bool,
    pub supports_seed_control: bool,
    pub supports_workflow_graph: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CapabilityRegistrySnapshot {
    pub mcp: Vec<McpCapabilityDescriptor>,
    pub skills: Vec<SkillCapabilityDescriptor>,
    pub agent_data_protocols: Vec<AgentDataCapabilityDescriptor>,
    pub asr_backends: Vec<AsrCapabilityDescriptor>,
    pub alignment_backends: Vec<AlignmentCapabilityDescriptor>,
    pub generative_backends: Vec<GenerativeBackendCapabilityDescriptor>,
    pub agent_modes: Vec<AgentModeDescriptor>,
    pub subagents: Vec<SubagentCapabilityDescriptor>,
    pub surfaces: Vec<SessionSurfaceDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContinueRequest {
    pub thread_id: String,
    pub turn_id: String,
    pub objective: String,
    pub constraints: Vec<String>,
    pub continue_reason: ContinueReason,
    pub recent_state_changes: Vec<StateChangeKind>,
    pub working_hypothesis: Option<String>,
    pub last_agent_message: Option<String>,
    pub pending_input_count: usize,
    pub trigger_turn_pending: bool,
    pub tool_call_count_this_turn: usize,
    pub context_pressure: ContextPressure,
    pub duplicate_injection_detected: bool,
    pub repeated_follow_up_count: u32,
    pub progress: ProgressSnapshot,
    pub recovery: RecoverySnapshot,
    pub last_checkpoint: Option<CheckpointRef>,
    pub self_correction: SelfCorrectionState,
    pub post_compaction: PostCompactionState,
    pub runtime_mode: RuntimeModeState,
    pub safety: PermissionContextSnapshot,
    pub codex_continuation: Option<CodexSessionContinuationSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContinueMode {
    SameTurn,
    NextTurn,
    QueueOnly,
    Handoff,
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContinueDecision {
    pub allow_continue: bool,
    pub mode: ContinueMode,
    pub rationale: String,
    pub next_action: Option<String>,
    pub suppress_duplicate_injection: bool,
    pub downgrade_trigger_turn: bool,
    pub request_compaction: bool,
    pub request_handoff_summary: bool,
    pub request_checkpoint: bool,
    pub enter_self_correction: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannerRequest {
    pub objective: String,
    pub constraints: Vec<String>,
    pub confirmed_facts: Vec<String>,
    pub open_questions: Vec<String>,
    pub continue_reason: ContinueReason,
    pub context_pressure: ContextPressure,
    pub media_context: Vec<MediaContextEnvelope>,
    pub capability_registry: CapabilityRegistrySnapshot,
    pub progress: ProgressSnapshot,
    pub recovery: RecoverySnapshot,
    pub self_correction: SelfCorrectionState,
    pub post_compaction: PostCompactionState,
    pub runtime_mode: RuntimeModeState,
    pub safety: PermissionContextSnapshot,
    pub codex_continuation: Option<CodexSessionContinuationSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlannerResponse {
    pub recommended_mode: ContinueMode,
    pub recommended_next_action: Option<String>,
    pub rationale: String,
    pub confidence: f32,
    pub suggested_model: Option<ModelSelectionDecision>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continue_request_round_trip_json() {
        let request = ContinueRequest {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            objective: "fix the runtime loop".to_string(),
            constraints: vec!["do not lose context".to_string()],
            continue_reason: ContinueReason::PendingInput,
            recent_state_changes: vec![StateChangeKind::NewConfirmedFinding],
            working_hypothesis: Some("pending input is rearming the loop".to_string()),
            last_agent_message: Some("checking runtime state".to_string()),
            pending_input_count: 2,
            trigger_turn_pending: true,
            tool_call_count_this_turn: 3,
            context_pressure: ContextPressure {
                model_context_window: Some(128_000),
                active_context_tokens: Some(77_000),
                auto_compact_scope_tokens: Some(12_000),
                auto_compact_limit: Some(96_000),
                tokens_until_compaction: Some(19_000),
                recent_compaction_count: 1,
                last_compaction_had_state_change: false,
            },
            duplicate_injection_detected: true,
            repeated_follow_up_count: 4,
            progress: ProgressSnapshot {
                completed_steps: 2,
                total_steps_hint: Some(5),
                last_meaningful_progress_at_ms: Some(1_700_000_000_000),
                no_progress_iteration_count: 0,
            },
            recovery: RecoverySnapshot {
                last_recovery_reason: Some(RecoveryReason::PartialResult),
                recovery_attempt_count: 1,
                last_recovery_had_progress: true,
            },
            last_checkpoint: Some(CheckpointRef {
                checkpoint_id: "cp-1".to_string(),
                thread_id: "thread-1".to_string(),
                turn_id: "turn-0".to_string(),
                created_at_ms: 1_700_000_000_000,
                summary: "captured initial progress".to_string(),
            }),
            self_correction: SelfCorrectionState {
                active: false,
                correction_attempt_count: 0,
                last_correction_target: None,
                last_correction_had_progress: false,
            },
            post_compaction: PostCompactionState {
                pending_post_compaction: true,
                first_post_compaction_turn: false,
                compaction_window_id: Some("window-7".to_string()),
                last_compaction_checkpoint_id: Some("cp-1".to_string()),
            },
            runtime_mode: RuntimeModeState {
                active_agent_mode: Some("build".to_string()),
                previous_agent_mode: Some("plan".to_string()),
                mode_transition_pending_guidance: false,
                invoked_skills: vec![SkillInvocationState {
                    skill_id: "context-budget-gate".to_string(),
                    invocation_ref: Some("skill://gate/1".to_string()),
                    invoked_at_ms: Some(1_700_000_000_123),
                }],
            },
            safety: PermissionContextSnapshot {
                sandbox_mode: SandboxModeDescriptor::WorkspaceWrite,
                approval_policy: ApprovalPolicyDescriptor::OnRequest,
                permission_profile_summary: "managed".to_string(),
                network_access_enabled: false,
                requires_explicit_approval_for_next_step: false,
            },
            codex_continuation: Some(CodexSessionContinuationSnapshot {
                source_thread_id: "thread-1".to_string(),
                source_turn_id: "turn-0".to_string(),
                source_model: "gpt-5.4".to_string(),
                source_provider: "llm-router".to_string(),
                active_agent_mode: Some("build".to_string()),
                safety: PermissionContextSnapshot {
                    sandbox_mode: SandboxModeDescriptor::WorkspaceWrite,
                    approval_policy: ApprovalPolicyDescriptor::OnRequest,
                    permission_profile_summary: "managed".to_string(),
                    network_access_enabled: false,
                    requires_explicit_approval_for_next_step: false,
                },
                reference_context_present: true,
            }),
        };

        let json = serde_json::to_string(&request).expect("serialize request");
        let decoded: ContinueRequest = serde_json::from_str(&json).expect("deserialize request");
        assert_eq!(decoded, request);
    }

    #[test]
    fn media_context_envelope_round_trip_json() {
        let envelope = MediaContextEnvelope {
            asset: MediaAssetRef {
                asset_id: "media-1".to_string(),
                kind: MediaKind::Video,
                source: MediaSource::LocalPath {
                    path: "clips/demo.mp4".to_string(),
                },
                display_name: Some("demo clip".to_string()),
            },
            derived_artifacts: vec![DerivedMediaArtifact {
                artifact_id: "artifact-1".to_string(),
                media_asset_id: "media-1".to_string(),
                kind: DerivedMediaArtifactKind::Transcript,
                time_range: Some(TimeRangeMs {
                    start_ms: 0,
                    end_ms: 10_000,
                }),
                summary: Some("opening segment transcript".to_string()),
                reference: Some("artifact://transcript/1".to_string()),
            }],
            hyperframes: vec![Hyperframe {
                hyperframe_id: "hf-1".to_string(),
                source_media_asset_id: "media-1".to_string(),
                time_range: TimeRangeMs {
                    start_ms: 0,
                    end_ms: 10_000,
                },
                transcript: Some("operator introduces the task".to_string()),
                subtitle_text: None,
                visual_summary: Some("terminal window and slide deck".to_string()),
                keyframe_artifact_ids: vec!["frame-001".to_string()],
                operator_annotations: vec!["important setup phase".to_string()],
                evidence_summary: "setup phase establishes the objective".to_string(),
            }],
            operator_summary: "video contains setup context".to_string(),
            model_summary: "use the setup phase as context".to_string(),
            token_budget_hint: Some(2048),
        };

        let json = serde_json::to_string(&envelope).expect("serialize media envelope");
        let decoded: MediaContextEnvelope =
            serde_json::from_str(&json).expect("deserialize media envelope");
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn model_selection_request_round_trip_json() {
        let request = ModelSelectionRequest {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-2".to_string(),
            intent: ModelSelectionIntent {
                required_input_modalities: vec![InputModality::Text, InputModality::Hyperframe],
                required_output_modalities: vec![
                    OutputModality::StructuredJson,
                    OutputModality::ToolCall,
                ],
                prefer_long_context: true,
                prefer_low_latency: false,
                prefer_low_cost: false,
                requires_tool_use: true,
                task_label: Some("video_analysis".to_string()),
                reasoning_mode: Some(ReasoningMode::SummaryOnly),
                structured_output_mode: Some(StructuredOutputMode::JsonSchema),
                execution_mode: Some(ExecutionMode::Streaming),
            },
            context_pressure: ContextPressure {
                model_context_window: Some(256_000),
                active_context_tokens: Some(120_000),
                auto_compact_scope_tokens: Some(18_000),
                auto_compact_limit: Some(192_000),
                tokens_until_compaction: Some(72_000),
                recent_compaction_count: 0,
                last_compaction_had_state_change: true,
            },
            media_assets: vec![MediaAssetRef {
                asset_id: "video-1".to_string(),
                kind: MediaKind::Video,
                source: MediaSource::LocalPath {
                    path: "videos/demo.mp4".to_string(),
                },
                display_name: Some("demo".to_string()),
            }],
        };

        let json = serde_json::to_string(&request).expect("serialize model selection request");
        let decoded: ModelSelectionRequest =
            serde_json::from_str(&json).expect("deserialize model selection request");
        assert_eq!(decoded, request);
    }
}
