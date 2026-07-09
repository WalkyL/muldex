use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde::Serialize;

use crate::runtime::RuntimeCommand;
use crate::runtime::RuntimeCommandResult;
use crate::runtime::RuntimeDriver;
use crate::runtime::RuntimeState;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RuntimeHostError {
    #[error("session already exists: {0}")]
    SessionAlreadyExists(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("snapshot serialization failed: {0}")]
    SnapshotSerialization(String),
    #[error("snapshot IO failed: {0}")]
    SnapshotIo(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSessionSummary {
    pub session_id: String,
    pub thread_id: String,
    pub cycle_index: u32,
    pub phase: crate::runtime::RuntimePhase,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeSessionSnapshot {
    pub session_id: String,
    pub driver: RuntimeDriver,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RuntimeHostSnapshot {
    pub sessions: Vec<RuntimeSessionSnapshot>,
}

#[derive(Debug, Default)]
pub struct RuntimeHost {
    sessions: HashMap<String, RuntimeDriver>,
}

impl RuntimeHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_session(
        &mut self,
        session_id: impl Into<String>,
        state: RuntimeState,
    ) -> Result<(), RuntimeHostError> {
        let session_id = session_id.into();
        if self.sessions.contains_key(&session_id) {
            return Err(RuntimeHostError::SessionAlreadyExists(session_id));
        }

        self.sessions
            .insert(session_id, RuntimeDriver::new(state));
        Ok(())
    }

    pub fn apply_command(
        &mut self,
        session_id: &str,
        command: RuntimeCommand,
    ) -> Result<RuntimeCommandResult, RuntimeHostError> {
        let driver = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| RuntimeHostError::SessionNotFound(session_id.to_string()))?;
        Ok(driver.apply_command(command))
    }

    pub fn get_state(&self, session_id: &str) -> Result<&RuntimeState, RuntimeHostError> {
        self.sessions
            .get(session_id)
            .map(|driver| &driver.state)
            .ok_or_else(|| RuntimeHostError::SessionNotFound(session_id.to_string()))
    }

    pub fn list_sessions(&self) -> Vec<RuntimeSessionSummary> {
        let mut sessions = self
            .sessions
            .iter()
            .map(|(session_id, driver)| RuntimeSessionSummary {
                session_id: session_id.clone(),
                thread_id: driver.state.request.thread_id.clone(),
                cycle_index: driver.state.cycle_index,
                phase: driver.state.phase.clone(),
            })
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| left.session_id.cmp(&right.session_id));
        sessions
    }

    pub fn export_session(
        &self,
        session_id: &str,
    ) -> Result<RuntimeSessionSnapshot, RuntimeHostError> {
        let driver = self
            .sessions
            .get(session_id)
            .ok_or_else(|| RuntimeHostError::SessionNotFound(session_id.to_string()))?;
        Ok(RuntimeSessionSnapshot {
            session_id: session_id.to_string(),
            driver: driver.clone(),
        })
    }

    pub fn export_snapshot(&self) -> RuntimeHostSnapshot {
        let mut sessions = self
            .sessions
            .iter()
            .map(|(session_id, driver)| RuntimeSessionSnapshot {
                session_id: session_id.clone(),
                driver: driver.clone(),
            })
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| left.session_id.cmp(&right.session_id));
        RuntimeHostSnapshot { sessions }
    }

    pub fn import_snapshot(snapshot: RuntimeHostSnapshot) -> Result<Self, RuntimeHostError> {
        let mut host = RuntimeHost::new();
        for session in snapshot.sessions {
            host.create_session(session.session_id, session.driver.state)?;
        }
        Ok(host)
    }

    pub fn save_snapshot_to_path(&self, path: impl AsRef<Path>) -> Result<(), RuntimeHostError> {
        let snapshot = self.export_snapshot();
        let json = serde_json::to_string_pretty(&snapshot)
            .map_err(|error| RuntimeHostError::SnapshotSerialization(error.to_string()))?;
        fs::write(path, json).map_err(|error| RuntimeHostError::SnapshotIo(error.to_string()))
    }

    pub fn load_snapshot_from_path(path: impl AsRef<Path>) -> Result<Self, RuntimeHostError> {
        let json = fs::read_to_string(path)
            .map_err(|error| RuntimeHostError::SnapshotIo(error.to_string()))?;
        let snapshot: RuntimeHostSnapshot = serde_json::from_str(&json)
            .map_err(|error| RuntimeHostError::SnapshotSerialization(error.to_string()))?;
        Self::import_snapshot(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RuntimeCommand;
    use crate::runtime::RuntimeCommandResult;
    use crate::runtime::RuntimeEvent;
    use crate::runtime::RuntimePhase;
    use muldex_core::protocol::ApprovalPolicyDescriptor;
    use muldex_core::protocol::ContextPressure;
    use muldex_core::protocol::ContinueDecision;
    use muldex_core::protocol::ContinueMode;
    use muldex_core::protocol::ContinueReason;
    use muldex_core::protocol::ContinueRequest;
    use muldex_core::protocol::ExecutionMode;
    use muldex_core::protocol::InterruptQueueState;
    use muldex_core::protocol::PermissionContextSnapshot;
    use muldex_core::protocol::PendingApprovalState;
    use muldex_core::protocol::PostCompactionState;
    use muldex_core::protocol::ProgressSnapshot;
    use muldex_core::protocol::RecoverySnapshot;
    use muldex_core::protocol::RuntimeModeState;
    use muldex_core::protocol::SandboxModeDescriptor;
    use muldex_core::protocol::SelfCorrectionState;
    use muldex_core::protocol::StateChangeKind;

    fn sample_state(session_suffix: &str) -> RuntimeState {
        RuntimeState {
            request: ContinueRequest {
                thread_id: format!("thread-{session_suffix}"),
                turn_id: "turn-1".to_string(),
                objective: "continue task".to_string(),
                constraints: vec!["do not spin".to_string()],
                continue_reason: ContinueReason::PendingInput,
                recent_state_changes: vec![StateChangeKind::NewConfirmedFinding],
                working_hypothesis: None,
                last_agent_message: None,
                pending_input_count: 0,
                trigger_turn_pending: false,
                tool_call_count_this_turn: 0,
                context_pressure: ContextPressure::default(),
                duplicate_injection_detected: false,
                repeated_follow_up_count: 0,
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
                post_compaction: PostCompactionState::default(),
                runtime_mode: RuntimeModeState {
                    active_agent_mode: Some("build".to_string()),
                    previous_agent_mode: None,
                    active_execution_mode: Some(ExecutionMode::Interactive),
                    previous_execution_mode: None,
                    mode_transition_pending_guidance: false,
                    invoked_skills: Vec::new(),
                },
                pending_approval: PendingApprovalState::default(),
                interrupts: InterruptQueueState::default(),
                last_run_report: None,
                safety: PermissionContextSnapshot {
                    sandbox_mode: SandboxModeDescriptor::WorkspaceWrite,
                    approval_policy: ApprovalPolicyDescriptor::OnRequest,
                    permission_profile_summary: "managed".to_string(),
                    network_access_enabled: false,
                    requires_explicit_approval_for_next_step: false,
                },
                codex_continuation: None,
            },
            cycle_index: 0,
            phase: RuntimePhase::Ready,
            latest_report: None,
        }
    }

    #[test]
    fn host_can_create_and_list_sessions() {
        let mut host = RuntimeHost::new();
        host.create_session("session-b", sample_state("b"))
            .expect("create session b");
        host.create_session("session-a", sample_state("a"))
            .expect("create session a");

        let sessions = host.list_sessions();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].session_id, "session-a");
        assert_eq!(sessions[1].session_id, "session-b");
    }

    #[test]
    fn host_routes_commands_and_persists_state() {
        let mut host = RuntimeHost::new();
        host.create_session("session-1", sample_state("1"))
            .expect("create session");

        let result = host
            .apply_command(
                "session-1",
                RuntimeCommand::Decision(ContinueDecision {
                    allow_continue: true,
                    mode: ContinueMode::NextTurn,
                    rationale: "advance one bounded cycle".to_string(),
                    next_action: None,
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
            )
            .expect("apply command");

        match result {
            RuntimeCommandResult::Step(step) => {
                assert_eq!(step.updated_state.cycle_index, 1);
            }
            _ => panic!("expected step result"),
        }

        let state = host.get_state("session-1").expect("session state");
        assert_eq!(state.cycle_index, 1);
        assert_eq!(state.phase, RuntimePhase::Running);
        assert!(state.latest_report.is_some());
    }

    #[test]
    fn host_reports_missing_session_errors() {
        let mut host = RuntimeHost::new();
        let error = host
            .apply_command(
                "missing",
                RuntimeCommand::Event(RuntimeEvent::MarkCompleted),
            )
            .expect_err("missing session should fail");

        assert_eq!(error, RuntimeHostError::SessionNotFound("missing".to_string()));
    }

    #[test]
    fn host_rejects_duplicate_session_ids() {
        let mut host = RuntimeHost::new();
        host.create_session("session-1", sample_state("1"))
            .expect("create first session");
        let error = host
            .create_session("session-1", sample_state("other"))
            .expect_err("duplicate session should fail");

        assert_eq!(
            error,
            RuntimeHostError::SessionAlreadyExists("session-1".to_string())
        );
    }

    #[test]
    fn host_snapshot_round_trip_preserves_sessions() {
        let mut host = RuntimeHost::new();
        host.create_session("session-a", sample_state("a"))
            .expect("create session a");
        host.create_session("session-b", sample_state("b"))
            .expect("create session b");

        let snapshot = host.export_snapshot();
        let restored = RuntimeHost::import_snapshot(snapshot).expect("restore snapshot");

        let sessions = restored.list_sessions();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].session_id, "session-a");
        assert_eq!(sessions[1].session_id, "session-b");
    }

    #[test]
    fn restored_host_can_continue_executing_commands() {
        let mut host = RuntimeHost::new();
        host.create_session("session-1", sample_state("1"))
            .expect("create session");

        let snapshot = host.export_snapshot();
        let mut restored = RuntimeHost::import_snapshot(snapshot).expect("restore snapshot");

        let result = restored
            .apply_command(
                "session-1",
                RuntimeCommand::Decision(ContinueDecision {
                    allow_continue: true,
                    mode: ContinueMode::NextTurn,
                    rationale: "resume from restored host state".to_string(),
                    next_action: None,
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
            )
            .expect("apply command after restore");

        match result {
            RuntimeCommandResult::Step(step) => {
                assert_eq!(step.updated_state.cycle_index, 1);
            }
            _ => panic!("expected step result after restore"),
        }
    }

    #[test]
    fn host_can_round_trip_through_file_backed_snapshot_api() {
        let mut host = RuntimeHost::new();
        host.create_session("session-1", sample_state("1"))
            .expect("create session");

        let temp_dir = std::env::temp_dir();
        let snapshot_path = temp_dir.join("muldex-runtime-host-snapshot-test.json");

        host.save_snapshot_to_path(&snapshot_path)
            .expect("save snapshot");
        let restored = RuntimeHost::load_snapshot_from_path(&snapshot_path)
            .expect("load snapshot");

        let state = restored.get_state("session-1").expect("restored state");
        assert_eq!(state.request.thread_id, "thread-1");

        let _ = fs::remove_file(&snapshot_path);
    }
}
