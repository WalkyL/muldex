use serde::Deserialize;
use serde::Serialize;

use crate::protocol::CapabilityRegistrySnapshot;
use crate::protocol::CheckpointRef;
use crate::protocol::ContinueMode;
use crate::protocol::ContextPressure;
use crate::protocol::MediaContextEnvelope;
use crate::protocol::ProgressSnapshot;
use crate::protocol::RecoverySnapshot;
use crate::protocol::SelfCorrectionState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProhibitionRule {
    NoFakeProgress,
    NoDuplicateInjection,
    NoSilentScopeExpansion,
    NoUnverifiedCompletion,
    NoRepeatedNoProgressContinuation,
    NoSilentModelSwitch,
    NoSilentExecutionModeSwitch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EscalationPolicy {
    pub no_progress_limit: u32,
    pub repeated_compaction_limit: u32,
    pub self_correction_limit: u32,
    pub request_checkpoint_before_handoff: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReasoningHarnessRequest {
    pub objective: String,
    pub constraints: Vec<String>,
    pub evidence_scope: Vec<String>,
    pub allowed_capability_classes: Vec<String>,
    pub prohibited_behaviors: Vec<ProhibitionRule>,
    pub progress: ProgressSnapshot,
    pub recovery: RecoverySnapshot,
    pub last_checkpoint: Option<CheckpointRef>,
    pub self_correction: SelfCorrectionState,
    pub context_pressure: ContextPressure,
    pub media_context: Vec<MediaContextEnvelope>,
    pub capability_registry: CapabilityRegistrySnapshot,
    pub escalation_policy: EscalationPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReasoningHarnessDecision {
    pub mode: ContinueMode,
    pub rationale: String,
    pub should_checkpoint: bool,
    pub should_enter_self_correction: bool,
    pub violated_rules: Vec<ProhibitionRule>,
}

pub fn decide_reasoning_harness(request: &ReasoningHarnessRequest) -> ReasoningHarnessDecision {
    let no_progress_violation = request.progress.no_progress_iteration_count
        >= request.escalation_policy.no_progress_limit;
    let self_correction_violation = request.self_correction.correction_attempt_count
        >= request.escalation_policy.self_correction_limit;
    let compaction_violation = request.context_pressure.recent_compaction_count
        >= request.escalation_policy.repeated_compaction_limit
        && !request.context_pressure.last_compaction_had_state_change;

    let mut violated_rules = Vec::new();
    if no_progress_violation {
        violated_rules.push(ProhibitionRule::NoRepeatedNoProgressContinuation);
    }
    if !request.recovery.last_recovery_had_progress && request.self_correction.active {
        violated_rules.push(ProhibitionRule::NoFakeProgress);
    }

    if no_progress_violation || self_correction_violation || compaction_violation {
        return ReasoningHarnessDecision {
            mode: ContinueMode::Handoff,
            rationale: "reasoning harness blocked further low-value continuation".to_string(),
            should_checkpoint: request.escalation_policy.request_checkpoint_before_handoff,
            should_enter_self_correction: false,
            violated_rules,
        };
    }

    let should_enter_self_correction = request.recovery.last_recovery_reason.is_some()
        && !request.recovery.last_recovery_had_progress
        && request.self_correction.correction_attempt_count
            < request.escalation_policy.self_correction_limit;

    ReasoningHarnessDecision {
        mode: if should_enter_self_correction {
            ContinueMode::SameTurn
        } else {
            ContinueMode::NextTurn
        },
        rationale: if should_enter_self_correction {
            "enter self-correction under harness policy".to_string()
        } else {
            "continue under harness policy".to_string()
        },
        should_checkpoint: request.progress.completed_steps > 0
            && request.progress.completed_steps % 5 == 0,
        should_enter_self_correction,
        violated_rules,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> ReasoningHarnessRequest {
        ReasoningHarnessRequest {
            objective: "finish the task".to_string(),
            constraints: vec!["do not spin".to_string()],
            evidence_scope: vec!["current repo only".to_string()],
            allowed_capability_classes: vec!["tool".to_string()],
            prohibited_behaviors: vec![
                ProhibitionRule::NoFakeProgress,
                ProhibitionRule::NoRepeatedNoProgressContinuation,
            ],
            progress: ProgressSnapshot {
                completed_steps: 1,
                total_steps_hint: Some(4),
                last_meaningful_progress_at_ms: Some(1),
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
            context_pressure: ContextPressure::default(),
            media_context: Vec::new(),
            capability_registry: CapabilityRegistrySnapshot::default(),
            escalation_policy: EscalationPolicy {
                no_progress_limit: 3,
                repeated_compaction_limit: 2,
                self_correction_limit: 2,
                request_checkpoint_before_handoff: true,
            },
        }
    }

    #[test]
    fn harness_enters_handoff_after_repeated_no_progress() {
        let mut request = base_request();
        request.progress.no_progress_iteration_count = 3;

        let decision = decide_reasoning_harness(&request);
        assert_eq!(decision.mode, ContinueMode::Handoff);
        assert!(decision.should_checkpoint);
        assert!(decision
            .violated_rules
            .contains(&ProhibitionRule::NoRepeatedNoProgressContinuation));
    }

    #[test]
    fn harness_enters_self_correction_after_recoverable_failure() {
        let mut request = base_request();
        request.recovery.last_recovery_reason = Some(crate::protocol::RecoveryReason::ToolFailure);
        request.recovery.last_recovery_had_progress = false;

        let decision = decide_reasoning_harness(&request);
        assert_eq!(decision.mode, ContinueMode::SameTurn);
        assert!(decision.should_enter_self_correction);
    }
}
