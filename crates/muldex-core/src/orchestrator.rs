use async_trait::async_trait;

use crate::protocol::ContinueDecision;
use crate::protocol::ContinueRequest;
use crate::protocol::ModelCapabilityDescriptor;
use crate::protocol::ModelSelectionDecision;
use crate::protocol::ModelSelectionRequest;
use crate::protocol::PlannerRequest;
use crate::protocol::PlannerResponse;

#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    #[error("planner unavailable: {0}")]
    PlannerUnavailable(String),
    #[error("orchestration failed: {0}")]
    Other(String),
}

#[async_trait]
pub trait AgentOrchestrator: Send + Sync {
    async fn decide_continue(
        &self,
        request: &ContinueRequest,
    ) -> Result<ContinueDecision, OrchestratorError>;

    async fn plan_next_action(
        &self,
        request: &PlannerRequest,
    ) -> Result<PlannerResponse, OrchestratorError>;
}

#[async_trait]
pub trait ModelRouter: Send + Sync {
    async fn list_capabilities(&self) -> Result<Vec<ModelCapabilityDescriptor>, OrchestratorError>;

    async fn select_model(
        &self,
        request: &ModelSelectionRequest,
    ) -> Result<ModelSelectionDecision, OrchestratorError>;
}
