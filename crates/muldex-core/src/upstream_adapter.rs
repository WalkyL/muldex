use serde::Deserialize;
use serde::Serialize;

use crate::protocol::CapabilityRegistrySnapshot;
use crate::protocol::CheckpointRef;
use crate::protocol::ContextPressure;
use crate::protocol::ContinueReason;
use crate::protocol::PostCompactionState;
use crate::protocol::ProgressSnapshot;
use crate::protocol::RecoveryReason;
use crate::protocol::RecoverySnapshot;
use crate::protocol::RuntimeModeState;
use crate::protocol::SelfCorrectionState;
use crate::protocol::StateChangeKind;
use crate::reasoning_harness::EscalationPolicy;
use crate::reasoning_harness::ProhibitionRule;
use crate::reasoning_harness::ReasoningHarnessRequest;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexSignalSnapshot {
    pub thread_id: String,
    pub turn_id: String,
    pub objective: String,
    pub constraints: Vec<String>,
    pub working_hypothesis: Option<String>,
    pub last_agent_message: Option<String>,
    pub continue_reason: ContinueReason,
    pub recent_state_changes: Vec<StateChangeKind>,
    pub pending_input_count: usize,
    pub trigger_turn_pending: bool,
    pub tool_call_count_this_turn: usize,
    pub duplicate_injection_detected: bool,
    pub repeated_follow_up_count: u32,
    pub model_context_window: Option<u32>,
    pub active_context_tokens: Option<u32>,
    pub auto_compact_scope_tokens: Option<u32>,
    pub auto_compact_limit: Option<u32>,
    pub tokens_until_compaction: Option<u32>,
    pub recent_compaction_count: u32,
    pub last_compaction_had_state_change: bool,
    pub pending_post_compaction: bool,
    pub first_post_compaction_turn: bool,
    pub compaction_window_id: Option<String>,
    pub last_compaction_checkpoint_id: Option<String>,
    pub completed_steps: u32,
    pub total_steps_hint: Option<u32>,
    pub no_progress_iteration_count: u32,
    pub recovery_attempt_count: u32,
    pub last_recovery_reason: Option<RecoveryReason>,
    pub last_recovery_had_progress: bool,
    pub self_correction_active: bool,
    pub self_correction_attempt_count: u32,
    pub last_correction_target: Option<String>,
    pub last_correction_had_progress: bool,
    pub active_agent_mode: Option<String>,
    pub previous_agent_mode: Option<String>,
    pub mode_transition_pending_guidance: bool,
    pub invoked_skills: Vec<String>,
    pub capability_registry: CapabilityRegistrySnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexBootstrapSnapshot {
    pub thread_id: String,
    pub turn_id: String,
    pub cwd: String,
    pub model: String,
    pub model_provider: String,
    pub collaboration_mode: String,
    pub personality: Option<String>,
    pub approval_policy: String,
    pub permission_profile: String,
    pub service_tier: Option<String>,
    pub show_raw_agent_reasoning: bool,
    pub model_context_window: Option<u32>,
    pub auto_compact_token_limit: Option<u32>,
    pub auto_compact_token_limit_scope: String,
    pub reference_context_present: bool,
    pub prompt_input_count: usize,
    pub input_modalities: Vec<String>,
    pub tools_visible_count: usize,
    pub prompt_preview_text_items: usize,
}

pub fn codex_snapshot_to_harness_request(
    snapshot: CodexSignalSnapshot,
) -> ReasoningHarnessRequest {
    let checkpoint_id_for_ref = snapshot.last_compaction_checkpoint_id.clone();

    ReasoningHarnessRequest {
        objective: snapshot.objective,
        constraints: snapshot.constraints,
        evidence_scope: vec!["upstream codex signal snapshot".to_string()],
        allowed_capability_classes: vec![
            "tool".to_string(),
            "skill".to_string(),
            "mcp".to_string(),
        ],
        prohibited_behaviors: vec![
            ProhibitionRule::NoFakeProgress,
            ProhibitionRule::NoRepeatedNoProgressContinuation,
            ProhibitionRule::NoDuplicateInjection,
            ProhibitionRule::NoUnverifiedCompletion,
        ],
        progress: ProgressSnapshot {
            completed_steps: snapshot.completed_steps,
            total_steps_hint: snapshot.total_steps_hint,
            last_meaningful_progress_at_ms: None,
            no_progress_iteration_count: snapshot.no_progress_iteration_count,
        },
        recovery: RecoverySnapshot {
            last_recovery_reason: snapshot.last_recovery_reason,
            recovery_attempt_count: snapshot.recovery_attempt_count,
            last_recovery_had_progress: snapshot.last_recovery_had_progress,
        },
        last_checkpoint: checkpoint_id_for_ref.map(|checkpoint_id| CheckpointRef {
            checkpoint_id,
            thread_id: snapshot.thread_id.clone(),
            turn_id: snapshot.turn_id.clone(),
            created_at_ms: 0,
            summary: "derived from codex runtime snapshot".to_string(),
        }),
        self_correction: SelfCorrectionState {
            active: snapshot.self_correction_active,
            correction_attempt_count: snapshot.self_correction_attempt_count,
            last_correction_target: snapshot.last_correction_target,
            last_correction_had_progress: snapshot.last_correction_had_progress,
        },
        post_compaction: PostCompactionState {
            pending_post_compaction: snapshot.pending_post_compaction,
            first_post_compaction_turn: snapshot.first_post_compaction_turn,
            compaction_window_id: snapshot.compaction_window_id,
            last_compaction_checkpoint_id: snapshot.last_compaction_checkpoint_id,
        },
        runtime_mode: RuntimeModeState {
            active_agent_mode: snapshot.active_agent_mode,
            previous_agent_mode: snapshot.previous_agent_mode,
            mode_transition_pending_guidance: snapshot.mode_transition_pending_guidance,
            invoked_skills: snapshot
                .invoked_skills
                .into_iter()
                .map(|skill_id| crate::protocol::SkillInvocationState {
                    skill_id,
                    invocation_ref: None,
                    invoked_at_ms: None,
                })
                .collect(),
        },
        context_pressure: ContextPressure {
            model_context_window: snapshot.model_context_window,
            active_context_tokens: snapshot.active_context_tokens,
            auto_compact_scope_tokens: snapshot.auto_compact_scope_tokens,
            auto_compact_limit: snapshot.auto_compact_limit,
            tokens_until_compaction: snapshot.tokens_until_compaction,
            recent_compaction_count: snapshot.recent_compaction_count,
            last_compaction_had_state_change: snapshot.last_compaction_had_state_change,
        },
        media_context: Vec::new(),
        capability_registry: snapshot.capability_registry,
        escalation_policy: EscalationPolicy {
            no_progress_limit: 3,
            repeated_compaction_limit: 2,
            self_correction_limit: 2,
            request_checkpoint_before_handoff: true,
        },
    }
}

pub fn codex_bootstrap_snapshot_to_harness_request(
    snapshot: CodexBootstrapSnapshot,
) -> ReasoningHarnessRequest {
    let required_modalities = snapshot
        .input_modalities
        .iter()
        .map(|modality| modality.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mentions_image = required_modalities.iter().any(|item| item.contains("image"));

    ReasoningHarnessRequest {
        objective: format!(
            "continue useful work in workspace {} from codex bootstrap state",
            snapshot.cwd
        ),
        constraints: vec![
            "do not spin".to_string(),
            "respect codex-compatible sandbox and approval semantics".to_string(),
        ],
        evidence_scope: vec![
            format!("cwd: {}", snapshot.cwd),
            format!("model: {}", snapshot.model),
            format!("provider: {}", snapshot.model_provider),
            format!("collaboration_mode: {}", snapshot.collaboration_mode),
            format!("reference_context_present: {}", snapshot.reference_context_present),
            format!("approval_policy: {}", snapshot.approval_policy),
            format!("show_raw_agent_reasoning: {}", snapshot.show_raw_agent_reasoning),
        ],
        allowed_capability_classes: vec![
            "tool".to_string(),
            "skill".to_string(),
            "mcp".to_string(),
        ],
        prohibited_behaviors: vec![
            ProhibitionRule::NoFakeProgress,
            ProhibitionRule::NoRepeatedNoProgressContinuation,
            ProhibitionRule::NoSilentScopeExpansion,
        ],
        progress: ProgressSnapshot {
            completed_steps: 0,
            total_steps_hint: None,
            last_meaningful_progress_at_ms: None,
            no_progress_iteration_count: 0,
        },
        recovery: RecoverySnapshot {
            last_recovery_reason: None,
            recovery_attempt_count: 0,
            last_recovery_had_progress: true,
        },
        last_checkpoint: None,
        self_correction: SelfCorrectionState {
            active: false,
            correction_attempt_count: 0,
            last_correction_target: None,
            last_correction_had_progress: false,
        },
        post_compaction: PostCompactionState {
            pending_post_compaction: false,
            first_post_compaction_turn: false,
            compaction_window_id: None,
            last_compaction_checkpoint_id: None,
        },
        runtime_mode: RuntimeModeState {
            active_agent_mode: Some(snapshot.collaboration_mode),
            previous_agent_mode: None,
            mode_transition_pending_guidance: false,
            invoked_skills: Vec::new(),
        },
        context_pressure: ContextPressure {
            model_context_window: snapshot.model_context_window,
            active_context_tokens: None,
            auto_compact_scope_tokens: None,
            auto_compact_limit: snapshot.auto_compact_token_limit,
            tokens_until_compaction: None,
            recent_compaction_count: 0,
            last_compaction_had_state_change: true,
        },
        media_context: Vec::new(),
        capability_registry: CapabilityRegistrySnapshot::default(),
        escalation_policy: EscalationPolicy {
            no_progress_limit: 3,
            repeated_compaction_limit: 2,
            self_correction_limit: 2,
            request_checkpoint_before_handoff: true,
        },
    }
    .with_modalities_hint(mentions_image)
}

trait BootstrapHintExt {
    fn with_modalities_hint(self, mentions_image: bool) -> Self;
}

impl BootstrapHintExt for ReasoningHarnessRequest {
    fn with_modalities_hint(mut self, mentions_image: bool) -> Self {
        if mentions_image {
            self.constraints
                .push("image-capable model path may be required".to_string());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_snapshot_maps_into_harness_request() {
        let snapshot = CodexSignalSnapshot {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-9".to_string(),
            objective: "finish the task".to_string(),
            constraints: vec!["do not spin".to_string()],
            working_hypothesis: Some("pending input is rearming the loop".to_string()),
            last_agent_message: Some("checking runtime state".to_string()),
            continue_reason: ContinueReason::PendingInput,
            recent_state_changes: vec![StateChangeKind::NoMeaningfulChange],
            pending_input_count: 2,
            trigger_turn_pending: true,
            tool_call_count_this_turn: 4,
            duplicate_injection_detected: true,
            repeated_follow_up_count: 3,
            model_context_window: Some(256_000),
            active_context_tokens: Some(150_000),
            auto_compact_scope_tokens: Some(30_000),
            auto_compact_limit: Some(192_000),
            tokens_until_compaction: Some(42_000),
            recent_compaction_count: 2,
            last_compaction_had_state_change: false,
            pending_post_compaction: true,
            first_post_compaction_turn: false,
            compaction_window_id: Some("window-3".to_string()),
            last_compaction_checkpoint_id: Some("cp-7".to_string()),
            completed_steps: 5,
            total_steps_hint: Some(9),
            no_progress_iteration_count: 2,
            recovery_attempt_count: 1,
            last_recovery_reason: Some(RecoveryReason::PartialResult),
            last_recovery_had_progress: false,
            self_correction_active: true,
            self_correction_attempt_count: 1,
            last_correction_target: Some("retry failed step".to_string()),
            last_correction_had_progress: false,
            active_agent_mode: Some("build".to_string()),
            previous_agent_mode: Some("plan".to_string()),
            mode_transition_pending_guidance: false,
            invoked_skills: vec!["context-budget-gate".to_string()],
            capability_registry: CapabilityRegistrySnapshot::default(),
        };

        let request = codex_snapshot_to_harness_request(snapshot);
        assert_eq!(request.progress.completed_steps, 5);
        assert!(request.post_compaction.pending_post_compaction);
        assert_eq!(request.runtime_mode.invoked_skills.len(), 1);
        assert_eq!(request.context_pressure.recent_compaction_count, 2);
    }

    #[test]
    fn bootstrap_snapshot_maps_into_harness_request() {
        let snapshot = CodexBootstrapSnapshot {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            cwd: "/workspace".to_string(),
            model: "gpt-5.4".to_string(),
            model_provider: "llm-router".to_string(),
            collaboration_mode: "build".to_string(),
            personality: Some("pragmatic".to_string()),
            approval_policy: "OnRequest".to_string(),
            permission_profile: "managed".to_string(),
            service_tier: None,
            show_raw_agent_reasoning: false,
            model_context_window: Some(258_400),
            auto_compact_token_limit: Some(193_800),
            auto_compact_token_limit_scope: "body_after_prefix".to_string(),
            reference_context_present: true,
            prompt_input_count: 3,
            input_modalities: vec!["Text".to_string(), "Image".to_string()],
            tools_visible_count: 11,
            prompt_preview_text_items: 3,
        };

        let request = codex_bootstrap_snapshot_to_harness_request(snapshot);
        assert_eq!(request.runtime_mode.active_agent_mode.as_deref(), Some("build"));
        assert!(request
            .constraints
            .iter()
            .any(|line| line.contains("image-capable model path")));
        assert!(request
            .evidence_scope
            .iter()
            .any(|line| line.contains("reference_context_present: true")));
    }
}
