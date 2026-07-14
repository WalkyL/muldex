use async_trait::async_trait;

use crate::protocol::ContinueDecision;
use crate::protocol::ContinueMode;
use crate::protocol::ContinueReason;
use crate::protocol::ContinueRequest;
use crate::protocol::ExecutionMode;
use crate::protocol::InputModality;
use crate::protocol::ModelCapabilityDescriptor;
use crate::protocol::ModelSelectionDecision;
use crate::protocol::ModelSelectionRequest;
use crate::protocol::OutputModality;
use crate::protocol::PlannerRequest;
use crate::protocol::PlannerResponse;
use crate::protocol::ReasoningMode;
use crate::protocol::StructuredOutputMode;

use crate::orchestrator::AgentOrchestrator;
use crate::orchestrator::ModelRouter;
use crate::orchestrator::OrchestratorError;

#[async_trait]
pub trait ContextGovernor: Send + Sync {
    async fn should_suppress_duplicate_injection(&self, request: &ContinueRequest) -> bool;
}

#[async_trait]
pub trait WakeupPolicy: Send + Sync {
    async fn should_downgrade_trigger_turn(&self, request: &ContinueRequest) -> bool;
}

#[async_trait]
pub trait ToolContinuationPolicy: Send + Sync {
    async fn should_request_handoff(&self, request: &ContinueRequest) -> bool;
}

#[derive(Debug, Default)]
pub struct DeterministicOrchestrator;

#[async_trait]
impl AgentOrchestrator for DeterministicOrchestrator {
    async fn decide_continue(
        &self,
        request: &ContinueRequest,
    ) -> Result<ContinueDecision, OrchestratorError> {
        let repeated_without_change = request
            .recent_state_changes
            .iter()
            .all(|change| matches!(change, crate::protocol::StateChangeKind::NoMeaningfulChange));

        if request.repeated_follow_up_count >= 3 && repeated_without_change {
            return Ok(ContinueDecision {
                allow_continue: false,
                mode: ContinueMode::Handoff,
                rationale: "repeated follow-up without a meaningful state change".to_string(),
                next_action: None,
                pause_for_approval: false,
                consume_interrupts_now: false,
                may_continue_other_work: false,
                suppress_duplicate_injection: true,
                downgrade_trigger_turn: true,
                request_compaction: false,
                request_handoff_summary: true,
                request_checkpoint: true,
                enter_self_correction: false,
            });
        }

        if request.progress.no_progress_iteration_count >= 3
            && !request.recovery.last_recovery_had_progress
        {
            return Ok(ContinueDecision {
                allow_continue: false,
                mode: ContinueMode::Handoff,
                rationale: "long-running task stalled without measurable progress".to_string(),
                next_action: None,
                pause_for_approval: false,
                consume_interrupts_now: false,
                may_continue_other_work: false,
                suppress_duplicate_injection: request.duplicate_injection_detected,
                downgrade_trigger_turn: true,
                request_compaction: false,
                request_handoff_summary: true,
                request_checkpoint: true,
                enter_self_correction: false,
            });
        }

        if request.post_compaction.pending_post_compaction
            && request.progress.no_progress_iteration_count >= 2
        {
            return Ok(ContinueDecision {
                allow_continue: false,
                mode: ContinueMode::Handoff,
                rationale: "post-compaction turns are still failing to make progress".to_string(),
                next_action: None,
                pause_for_approval: false,
                consume_interrupts_now: false,
                may_continue_other_work: false,
                suppress_duplicate_injection: true,
                downgrade_trigger_turn: true,
                request_compaction: false,
                request_handoff_summary: true,
                request_checkpoint: true,
                enter_self_correction: false,
            });
        }

        Ok(ContinueDecision {
            allow_continue: true,
            mode: match request.continue_reason {
                ContinueReason::TriggerTurnWakeup if request.duplicate_injection_detected => {
                    ContinueMode::QueueOnly
                }
                _ => ContinueMode::NextTurn,
            },
            rationale: "default deterministic continuation policy".to_string(),
            next_action: request.working_hypothesis.clone(),
            pause_for_approval: false,
            consume_interrupts_now: false,
            may_continue_other_work: true,
            suppress_duplicate_injection: request.duplicate_injection_detected,
            downgrade_trigger_turn: false,
            request_compaction: false,
            request_handoff_summary: false,
            request_checkpoint: request.progress.completed_steps > 0
                && request.progress.completed_steps % 5 == 0,
            enter_self_correction: request.recovery.last_recovery_reason.is_some()
                && !request.recovery.last_recovery_had_progress,
        })
    }

    async fn plan_next_action(
        &self,
        request: &PlannerRequest,
    ) -> Result<PlannerResponse, OrchestratorError> {
        Ok(PlannerResponse {
            recommended_mode: ContinueMode::NextTurn,
            recommended_next_action: request.open_questions.first().cloned(),
            rationale: "default deterministic planner response".to_string(),
            confidence: 0.25,
            suggested_model: None,
        })
    }
}

#[async_trait]
impl ModelRouter for DeterministicOrchestrator {
    async fn list_capabilities(&self) -> Result<Vec<ModelCapabilityDescriptor>, OrchestratorError> {
        Ok(vec![ModelCapabilityDescriptor {
            model_id: "default-text".to_string(),
            provider_id: "default".to_string(),
            input_modalities: vec![InputModality::Text],
            output_modalities: vec![
                OutputModality::Text,
                OutputModality::StructuredJson,
                OutputModality::ToolCall,
            ],
            max_context_window: Some(128_000),
            supports_tool_use: true,
            supports_parallel_tool_use: true,
            supports_long_context: true,
            supports_streaming: true,
            supports_audio_input: false,
            supports_image_input: false,
            supports_video_context: false,
            supports_hyperframes: false,
            reasoning_modes: vec![ReasoningMode::Low, ReasoningMode::Medium],
            structured_output_modes: vec![
                StructuredOutputMode::Freeform,
                StructuredOutputMode::JsonSchema,
            ],
            execution_modes: vec![ExecutionMode::Interactive, ExecutionMode::Streaming],
            supports_citations: false,
            supports_prompt_caching: false,
        }])
    }

    async fn select_model(
        &self,
        request: &ModelSelectionRequest,
    ) -> Result<ModelSelectionDecision, OrchestratorError> {
        let needs_multimodal = request
            .intent
            .required_input_modalities
            .iter()
            .any(|modality| !matches!(modality, InputModality::Text));

        Ok(if needs_multimodal {
            ModelSelectionDecision {
                selected_model_id: "multimodal-default".to_string(),
                selected_provider_id: "default".to_string(),
                rationale: "non-text modality requested".to_string(),
                fallback_model_id: Some("default-text".to_string()),
            }
        } else {
            ModelSelectionDecision {
                selected_model_id: "default-text".to_string(),
                selected_provider_id: "default".to_string(),
                rationale: "text-only request".to_string(),
                fallback_model_id: None,
            }
        })
    }
}
