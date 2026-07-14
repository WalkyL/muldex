use serde::Deserialize;
use serde::Serialize;

use crate::protocol::CapabilityRegistrySnapshot;
use crate::protocol::CheckpointRef;
use crate::protocol::CodexSessionContinuationSnapshot;
use crate::protocol::ContextPressure;
use crate::protocol::ContinueMode;
use crate::protocol::InterruptInjectionMode;
use crate::protocol::InterruptQueueState;
use crate::protocol::MediaContextEnvelope;
use crate::protocol::PendingApprovalState;
use crate::protocol::PermissionContextSnapshot;
use crate::protocol::PostCompactionState;
use crate::protocol::ProgressSnapshot;
use crate::protocol::RecoverySnapshot;
use crate::protocol::RunReport;
use crate::protocol::RuntimeModeState;
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
    pub post_compaction: PostCompactionState,
    pub runtime_mode: RuntimeModeState,
    pub pending_approval: PendingApprovalState,
    pub interrupts: InterruptQueueState,
    pub last_run_report: Option<RunReport>,
    pub safety: PermissionContextSnapshot,
    pub codex_continuation: Option<CodexSessionContinuationSnapshot>,
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
    pub pause_for_approval: bool,
    pub consume_interrupts_now: bool,
    pub may_continue_other_work: bool,
    pub violated_rules: Vec<ProhibitionRule>,
}

pub fn decide_reasoning_harness(request: &ReasoningHarnessRequest) -> ReasoningHarnessDecision {
    let no_progress_violation =
        request.progress.no_progress_iteration_count >= request.escalation_policy.no_progress_limit;
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

    if request.pending_approval.blocked_on_approval {
        return ReasoningHarnessDecision {
            mode: if request.pending_approval.may_continue_other_work {
                ContinueMode::QueueOnly
            } else {
                ContinueMode::Handoff
            },
            rationale: if request.pending_approval.may_continue_other_work {
                "pause risky continuation and queue follow-up while awaiting approval".to_string()
            } else {
                "handoff because the run is blocked on approval and cannot continue other work"
                    .to_string()
            },
            should_checkpoint: request.escalation_policy.request_checkpoint_before_handoff,
            should_enter_self_correction: false,
            pause_for_approval: true,
            consume_interrupts_now: false,
            may_continue_other_work: request.pending_approval.may_continue_other_work,
            violated_rules,
        };
    }

    if no_progress_violation || self_correction_violation || compaction_violation {
        return ReasoningHarnessDecision {
            mode: ContinueMode::Handoff,
            rationale: "reasoning harness blocked further low-value continuation".to_string(),
            should_checkpoint: request.escalation_policy.request_checkpoint_before_handoff,
            should_enter_self_correction: false,
            pause_for_approval: false,
            consume_interrupts_now: false,
            may_continue_other_work: false,
            violated_rules,
        };
    }

    let should_enter_self_correction = request.recovery.last_recovery_reason.is_some()
        && !request.recovery.last_recovery_had_progress
        && request.self_correction.correction_attempt_count
            < request.escalation_policy.self_correction_limit
        && !request.safety.requires_explicit_approval_for_next_step;

    let has_immediate_safe_point_interrupt = request
        .interrupts
        .pending_interrupts
        .iter()
        .any(|interrupt| interrupt.injection_mode == InterruptInjectionMode::ImmediateSafePoint);

    let mode = if should_enter_self_correction || has_immediate_safe_point_interrupt {
        ContinueMode::SameTurn
    } else {
        ContinueMode::NextTurn
    };

    let rationale = if should_enter_self_correction {
        "enter self-correction under harness policy".to_string()
    } else if has_immediate_safe_point_interrupt {
        "continue at the current safe point to absorb queued interrupt state".to_string()
    } else {
        "continue under harness policy".to_string()
    };

    ReasoningHarnessDecision {
        mode,
        rationale,
        should_checkpoint: request.progress.completed_steps > 0
            && request.progress.completed_steps % 5 == 0,
        should_enter_self_correction,
        pause_for_approval: false,
        consume_interrupts_now: has_immediate_safe_point_interrupt,
        may_continue_other_work: true,
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
            post_compaction: PostCompactionState::default(),
            runtime_mode: RuntimeModeState::default(),
            pending_approval: PendingApprovalState::default(),
            interrupts: InterruptQueueState::default(),
            last_run_report: None,
            safety: PermissionContextSnapshot {
                sandbox_mode: crate::protocol::SandboxModeDescriptor::WorkspaceWrite,
                approval_policy: crate::protocol::ApprovalPolicyDescriptor::OnRequest,
                permission_profile_summary: "managed".to_string(),
                network_access_enabled: false,
                requires_explicit_approval_for_next_step: false,
            },
            codex_continuation: None,
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
        assert!(
            decision
                .violated_rules
                .contains(&ProhibitionRule::NoRepeatedNoProgressContinuation)
        );
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

    #[test]
    fn harness_queues_when_waiting_on_approval_but_other_work_is_allowed() {
        let mut request = base_request();
        request.pending_approval.active_request = Some(crate::protocol::PermissionRequest {
            request_id: "approval-1".to_string(),
            action_kind: crate::protocol::PermissionActionKind::RemoteMutation,
            summary: "open a pull request".to_string(),
            rationale: "publish the validated fix for review".to_string(),
            urgency: crate::protocol::PermissionUrgency::Normal,
            wait_for_decision: false,
            requested_at_ms: Some(1),
            expires_at_ms: None,
        });
        request.pending_approval.blocked_on_approval = true;
        request.pending_approval.may_continue_other_work = true;

        let decision = decide_reasoning_harness(&request);
        assert_eq!(decision.mode, ContinueMode::QueueOnly);
        assert!(!decision.should_enter_self_correction);
        assert!(decision.pause_for_approval);
        assert!(decision.may_continue_other_work);
    }

    #[test]
    fn harness_handoffs_when_waiting_on_approval_and_no_other_work_is_allowed() {
        let mut request = base_request();
        request.pending_approval.active_request = Some(crate::protocol::PermissionRequest {
            request_id: "approval-2".to_string(),
            action_kind: crate::protocol::PermissionActionKind::ExternalCommunication,
            summary: "message the operator".to_string(),
            rationale: "cannot continue until the operator responds".to_string(),
            urgency: crate::protocol::PermissionUrgency::High,
            wait_for_decision: true,
            requested_at_ms: Some(1),
            expires_at_ms: None,
        });
        request.pending_approval.blocked_on_approval = true;
        request.pending_approval.may_continue_other_work = false;

        let decision = decide_reasoning_harness(&request);
        assert_eq!(decision.mode, ContinueMode::Handoff);
        assert!(decision.should_checkpoint);
        assert!(decision.pause_for_approval);
        assert!(!decision.may_continue_other_work);
    }

    #[test]
    fn harness_prefers_same_turn_for_immediate_safe_point_interrupts() {
        let mut request = base_request();
        request
            .interrupts
            .pending_interrupts
            .push(crate::protocol::PendingInterrupt {
                interrupt_id: "interrupt-1".to_string(),
                kind: crate::protocol::InterruptKind::ApprovalDecision,
                summary: "approval decision arrived".to_string(),
                injection_mode: crate::protocol::InterruptInjectionMode::ImmediateSafePoint,
                created_at_ms: Some(1),
            });

        let decision = decide_reasoning_harness(&request);
        assert_eq!(decision.mode, ContinueMode::SameTurn);
        assert!(decision.consume_interrupts_now);
        assert!(decision.rationale.contains("safe point"));
    }
}
