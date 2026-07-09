use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;

use crate::orchestrator::AgentOrchestrator;
use crate::orchestrator::OrchestratorError;
use crate::protocol::ContinueDecision;
use crate::protocol::ContinueRequest;
use crate::protocol::PlannerRequest;
use crate::protocol::PlannerResponse;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentlySidecarRequest {
    pub operation: String,
    pub continue_request: Option<ContinueRequest>,
    pub planner_request: Option<PlannerRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentlySidecarResponse {
    pub continue_decision: Option<ContinueDecision>,
    pub planner_response: Option<PlannerResponse>,
    pub error: Option<String>,
}

#[async_trait]
pub trait AgentlyTransport: Send + Sync {
    async fn invoke(
        &self,
        request: &AgentlySidecarRequest,
    ) -> Result<AgentlySidecarResponse, OrchestratorError>;
}

pub struct AgentlyOrchestrator<T: AgentlyTransport> {
    transport: T,
}

impl<T: AgentlyTransport> AgentlyOrchestrator<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl<T: AgentlyTransport> AgentOrchestrator for AgentlyOrchestrator<T> {
    async fn decide_continue(
        &self,
        request: &ContinueRequest,
    ) -> Result<ContinueDecision, OrchestratorError> {
        let response = self
            .transport
            .invoke(&AgentlySidecarRequest {
                operation: "decide_continue".to_string(),
                continue_request: Some(request.clone()),
                planner_request: None,
            })
            .await?;

        if let Some(error) = response.error {
            return Err(OrchestratorError::Other(error));
        }

        response.continue_decision.ok_or_else(|| {
            OrchestratorError::Other(
                "agently transport did not return a continue decision".to_string(),
            )
        })
    }

    async fn plan_next_action(
        &self,
        request: &PlannerRequest,
    ) -> Result<PlannerResponse, OrchestratorError> {
        let response = self
            .transport
            .invoke(&AgentlySidecarRequest {
                operation: "plan_next_action".to_string(),
                continue_request: None,
                planner_request: Some(request.clone()),
            })
            .await?;

        if let Some(error) = response.error {
            return Err(OrchestratorError::Other(error));
        }

        response.planner_response.ok_or_else(|| {
            OrchestratorError::Other(
                "agently transport did not return a planner response".to_string(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::DeterministicOrchestrator;
    use crate::protocol::CapabilityRegistrySnapshot;
    use crate::protocol::ContextPressure;
    use crate::protocol::ContinueMode;
    use crate::protocol::ContinueReason;
    use crate::protocol::ContinueRequest;
    use crate::protocol::ExecutionMode;
    use crate::protocol::InterruptQueueState;
    use crate::protocol::PendingApprovalState;
    use crate::protocol::PlannerRequest;
    use crate::protocol::StateChangeKind;
    use async_trait::async_trait;

    struct EchoTransport;

    #[async_trait]
    impl AgentlyTransport for EchoTransport {
        async fn invoke(
            &self,
            request: &AgentlySidecarRequest,
        ) -> Result<AgentlySidecarResponse, OrchestratorError> {
            match request.operation.as_str() {
                "decide_continue" => Ok(AgentlySidecarResponse {
                    continue_decision: Some(ContinueDecision {
                        allow_continue: true,
                        mode: ContinueMode::NextTurn,
                        rationale: "echo transport decision".to_string(),
                        next_action: request
                            .continue_request
                            .as_ref()
                            .and_then(|r| r.working_hypothesis.clone()),
                        pause_for_approval: false,
                        consume_interrupts_now: false,
                        may_continue_other_work: true,
                        suppress_duplicate_injection: false,
                        downgrade_trigger_turn: false,
                        request_compaction: false,
                        request_handoff_summary: false,
                        request_checkpoint: false,
                        enter_self_correction: false,
                    }),
                    planner_response: None,
                    error: None,
                }),
                "plan_next_action" => Ok(AgentlySidecarResponse {
                    continue_decision: None,
                    planner_response: Some(PlannerResponse {
                        recommended_mode: ContinueMode::NextTurn,
                        recommended_next_action: request
                            .planner_request
                            .as_ref()
                            .and_then(|r| r.open_questions.first().cloned()),
                        rationale: "echo transport planner".to_string(),
                        confidence: 0.5,
                        suggested_model: None,
                    }),
                    error: None,
                }),
                _ => Err(OrchestratorError::Other("unexpected operation".to_string())),
            }
        }
    }

    fn sample_continue_request() -> ContinueRequest {
        ContinueRequest {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            objective: "continue task".to_string(),
            constraints: vec!["do not spin".to_string()],
            continue_reason: ContinueReason::PendingInput,
            recent_state_changes: vec![StateChangeKind::NewConfirmedFinding],
            working_hypothesis: Some("retry with better context".to_string()),
            last_agent_message: None,
            pending_input_count: 1,
            trigger_turn_pending: false,
            tool_call_count_this_turn: 0,
            context_pressure: ContextPressure::default(),
            duplicate_injection_detected: false,
            repeated_follow_up_count: 0,
            progress: crate::protocol::ProgressSnapshot {
                completed_steps: 1,
                total_steps_hint: Some(3),
                last_meaningful_progress_at_ms: None,
                no_progress_iteration_count: 0,
            },
            recovery: crate::protocol::RecoverySnapshot {
                last_recovery_reason: None,
                recovery_attempt_count: 0,
                last_recovery_had_progress: true,
            },
            last_checkpoint: None,
            self_correction: crate::protocol::SelfCorrectionState {
                active: false,
                correction_attempt_count: 0,
                last_correction_target: None,
                last_correction_had_progress: false,
            },
            post_compaction: crate::protocol::PostCompactionState::default(),
            runtime_mode: crate::protocol::RuntimeModeState {
                active_execution_mode: Some(ExecutionMode::Interactive),
                ..crate::protocol::RuntimeModeState::default()
            },
            pending_approval: PendingApprovalState::default(),
            interrupts: InterruptQueueState::default(),
            last_run_report: None,
            safety: crate::protocol::PermissionContextSnapshot {
                sandbox_mode: crate::protocol::SandboxModeDescriptor::WorkspaceWrite,
                approval_policy: crate::protocol::ApprovalPolicyDescriptor::OnRequest,
                permission_profile_summary: "managed".to_string(),
                network_access_enabled: false,
                requires_explicit_approval_for_next_step: false,
            },
            codex_continuation: None,
        }
    }

    fn sample_planner_request() -> PlannerRequest {
        PlannerRequest {
            objective: "plan next step".to_string(),
            constraints: vec!["stay bounded".to_string()],
            confirmed_facts: vec!["one fact".to_string()],
            open_questions: vec!["what should run next?".to_string()],
            continue_reason: ContinueReason::PendingInput,
            context_pressure: ContextPressure::default(),
            media_context: Vec::new(),
            capability_registry: CapabilityRegistrySnapshot::default(),
            progress: crate::protocol::ProgressSnapshot {
                completed_steps: 1,
                total_steps_hint: Some(3),
                last_meaningful_progress_at_ms: None,
                no_progress_iteration_count: 0,
            },
            recovery: crate::protocol::RecoverySnapshot {
                last_recovery_reason: None,
                recovery_attempt_count: 0,
                last_recovery_had_progress: true,
            },
            self_correction: crate::protocol::SelfCorrectionState {
                active: false,
                correction_attempt_count: 0,
                last_correction_target: None,
                last_correction_had_progress: false,
            },
            post_compaction: crate::protocol::PostCompactionState::default(),
            runtime_mode: crate::protocol::RuntimeModeState {
                active_execution_mode: Some(ExecutionMode::Interactive),
                ..crate::protocol::RuntimeModeState::default()
            },
            pending_approval: PendingApprovalState::default(),
            interrupts: InterruptQueueState::default(),
            last_run_report: None,
            safety: crate::protocol::PermissionContextSnapshot {
                sandbox_mode: crate::protocol::SandboxModeDescriptor::WorkspaceWrite,
                approval_policy: crate::protocol::ApprovalPolicyDescriptor::OnRequest,
                permission_profile_summary: "managed".to_string(),
                network_access_enabled: false,
                requires_explicit_approval_for_next_step: false,
            },
            codex_continuation: None,
        }
    }

    #[tokio::test]
    async fn agently_orchestrator_routes_continue_requests() {
        let orchestrator = AgentlyOrchestrator::new(EchoTransport);
        let decision = orchestrator
            .decide_continue(&sample_continue_request())
            .await
            .expect("continue decision");

        assert_eq!(decision.mode, ContinueMode::NextTurn);
        assert_eq!(
            decision.next_action.as_deref(),
            Some("retry with better context")
        );
    }

    #[tokio::test]
    async fn agently_orchestrator_routes_planner_requests() {
        let orchestrator = AgentlyOrchestrator::new(EchoTransport);
        let response = orchestrator
            .plan_next_action(&sample_planner_request())
            .await
            .expect("planner response");

        assert_eq!(response.recommended_mode, ContinueMode::NextTurn);
        assert_eq!(
            response.recommended_next_action.as_deref(),
            Some("what should run next?")
        );
    }

    #[allow(dead_code)]
    fn _assert_deterministic_orchestrator_still_exists(_value: DeterministicOrchestrator) {}
}
