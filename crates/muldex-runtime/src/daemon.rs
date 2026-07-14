use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use crate::daemon_local::DaemonStateMetadata;
use crate::daemon_local::LocalDaemonError;
use crate::daemon_local::LocalDaemonOwnership;
use crate::daemon_local::StaleOwnershipReport;
use crate::daemon_transport::DaemonResponseEnvelope;
use crate::daemon_transport::DaemonTransportError;
use crate::daemon_transport::FileCommandTransport;
use crate::host::RuntimeHost;
use crate::host::RuntimeHostError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeDaemonStatus {
    Cold,
    Running,
    Stopped,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeDaemonError {
    #[error("host error: {0}")]
    Host(#[from] RuntimeHostError),
    #[error("local daemon error: {0}")]
    Local(#[from] LocalDaemonError),
    #[error("daemon transport error: {0}")]
    Transport(#[from] DaemonTransportError),
    #[error("daemon is not running")]
    NotRunning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDaemonLoopResult {
    pub iterations: usize,
    pub total_responses: usize,
}

#[derive(Debug)]
pub struct RuntimeDaemon {
    snapshot_path: PathBuf,
    status: RuntimeDaemonStatus,
    host: RuntimeHost,
    ownership: LocalDaemonOwnership,
    transport: FileCommandTransport,
}

impl RuntimeDaemon {
    pub fn new(snapshot_path: impl Into<PathBuf>) -> Self {
        let snapshot_path = snapshot_path.into();
        let runtime_root = match (
            snapshot_path.parent(),
            snapshot_path.file_stem().and_then(|stem| stem.to_str()),
        ) {
            (Some(parent), Some(stem)) => parent.join(format!("{stem}.muldex-daemon")),
            (Some(parent), None) => parent.join(".muldex-daemon"),
            (None, Some(stem)) => PathBuf::from(format!("{stem}.muldex-daemon")),
            (None, None) => PathBuf::from(".muldex-daemon"),
        };
        let transport_root = match (
            snapshot_path.parent(),
            snapshot_path.file_stem().and_then(|stem| stem.to_str()),
        ) {
            (Some(parent), Some(stem)) => parent.join(format!("{stem}.muldex-transport")),
            (Some(parent), None) => parent.join(".muldex-transport"),
            (None, Some(stem)) => PathBuf::from(format!("{stem}.muldex-transport")),
            (None, None) => PathBuf::from(".muldex-transport"),
        };
        Self {
            snapshot_path,
            status: RuntimeDaemonStatus::Cold,
            host: RuntimeHost::new(),
            ownership: LocalDaemonOwnership::new(runtime_root),
            transport: FileCommandTransport::new(transport_root),
        }
    }

    pub fn snapshot_path(&self) -> &Path {
        &self.snapshot_path
    }

    pub fn status(&self) -> &RuntimeDaemonStatus {
        &self.status
    }

    pub fn ownership(&self) -> &LocalDaemonOwnership {
        &self.ownership
    }

    pub fn stale_report(
        &self,
        stale_threshold_ms: u64,
    ) -> Result<StaleOwnershipReport, RuntimeDaemonError> {
        Ok(self.ownership.stale_report(now_ms(), stale_threshold_ms)?)
    }

    pub fn force_takeover(&mut self, stale_threshold_ms: u64) -> Result<(), RuntimeDaemonError> {
        let _ = self
            .ownership
            .force_takeover(current_pid(), now_ms(), stale_threshold_ms)?;
        self.write_state_metadata()?;
        Ok(())
    }

    pub fn transport(&self) -> &FileCommandTransport {
        &self.transport
    }

    pub fn boot_empty(&mut self) -> Result<(), RuntimeDaemonError> {
        self.host = RuntimeHost::new();
        self.ownership.acquire(current_pid(), now_ms())?;
        self.status = RuntimeDaemonStatus::Running;
        self.write_state_metadata()?;
        Ok(())
    }

    pub fn boot_from_disk_if_present(&mut self) -> Result<(), RuntimeDaemonError> {
        if self.snapshot_path.exists() {
            self.host = RuntimeHost::load_snapshot_from_path(&self.snapshot_path)?;
        } else {
            self.host = RuntimeHost::new();
        }
        self.ownership.acquire(current_pid(), now_ms())?;
        self.status = RuntimeDaemonStatus::Running;
        self.write_state_metadata()?;
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), RuntimeDaemonError> {
        if !matches!(self.status, RuntimeDaemonStatus::Running) {
            return Err(RuntimeDaemonError::NotRunning);
        }
        self.host.save_snapshot_to_path(&self.snapshot_path)?;
        self.ownership.refresh_heartbeat(now_ms())?;
        self.write_state_metadata()?;
        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<(), RuntimeDaemonError> {
        self.save()?;
        self.status = RuntimeDaemonStatus::Stopped;
        self.write_state_metadata()?;
        self.ownership.release()?;
        Ok(())
    }

    pub fn host(&self) -> Result<&RuntimeHost, RuntimeDaemonError> {
        if !matches!(self.status, RuntimeDaemonStatus::Running) {
            return Err(RuntimeDaemonError::NotRunning);
        }
        Ok(&self.host)
    }

    pub fn host_mut(&mut self) -> Result<&mut RuntimeHost, RuntimeDaemonError> {
        if !matches!(self.status, RuntimeDaemonStatus::Running) {
            return Err(RuntimeDaemonError::NotRunning);
        }
        Ok(&mut self.host)
    }

    pub fn process_transport_once(
        &mut self,
    ) -> Result<Vec<DaemonResponseEnvelope>, RuntimeDaemonError> {
        if !matches!(self.status, RuntimeDaemonStatus::Running) {
            return Err(RuntimeDaemonError::NotRunning);
        }
        let responses = self.transport.process_commands(&mut self.host)?;
        self.ownership.refresh_heartbeat(now_ms())?;
        self.write_state_metadata()?;
        Ok(responses)
    }

    pub fn serve_once(&mut self) -> Result<Vec<DaemonResponseEnvelope>, RuntimeDaemonError> {
        self.process_transport_once()
    }

    pub fn serve_loop(
        &mut self,
        iterations: usize,
    ) -> Result<RuntimeDaemonLoopResult, RuntimeDaemonError> {
        let mut total_responses = 0usize;
        for _ in 0..iterations {
            let responses = self.serve_once()?;
            total_responses = total_responses.saturating_add(responses.len());
        }
        Ok(RuntimeDaemonLoopResult {
            iterations,
            total_responses,
        })
    }

    fn write_state_metadata(&self) -> Result<(), RuntimeDaemonError> {
        let owner_pid = self
            .ownership
            .held_lock()
            .map(|metadata| metadata.owner_pid)
            .unwrap_or_else(current_pid);
        let metadata = DaemonStateMetadata {
            owner_pid,
            status: format!("{:?}", self.status),
            snapshot_path: self.snapshot_path.display().to_string(),
            session_count: self.host.list_sessions().len(),
            updated_at_ms: now_ms(),
        };
        self.ownership.write_state(&metadata)?;
        Ok(())
    }
}

fn current_pid() -> u32 {
    std::process::id()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_views::ClientResponsePayloadView;
    use crate::client_views::response_view;
    use crate::daemon_local::StaleOwnershipStatus;
    use crate::runtime::RuntimePhase;
    use crate::runtime::RuntimeState;
    use muldex_core::protocol::ApprovalPolicyDescriptor;
    use muldex_core::protocol::ContextPressure;
    use muldex_core::protocol::ContinueReason;
    use muldex_core::protocol::ContinueRequest;
    use muldex_core::protocol::ExecutionMode;
    use muldex_core::protocol::InterruptQueueState;
    use muldex_core::protocol::PendingApprovalState;
    use muldex_core::protocol::PermissionContextSnapshot;
    use muldex_core::protocol::PostCompactionState;
    use muldex_core::protocol::ProgressSnapshot;
    use muldex_core::protocol::RecoverySnapshot;
    use muldex_core::protocol::RuntimeModeState;
    use muldex_core::protocol::SandboxModeDescriptor;
    use muldex_core::protocol::SelfCorrectionState;
    use muldex_core::protocol::StateChangeKind;

    fn snapshot_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    fn cleanup(snapshot_path: &Path) {
        let runtime_root = snapshot_path
            .parent()
            .map(|parent| {
                let stem = snapshot_path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or(".muldex-daemon");
                parent.join(format!("{stem}.muldex-daemon"))
            })
            .unwrap_or_else(|| PathBuf::from(".muldex-daemon"));
        let _ = std::fs::remove_file(snapshot_path);
        let _ = std::fs::remove_dir_all(runtime_root);
    }

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
    fn daemon_can_boot_empty() {
        let path = snapshot_path("muldex-daemon-empty.json");
        cleanup(&path);
        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot empty daemon");

        assert_eq!(daemon.status(), &RuntimeDaemonStatus::Running);
        assert!(daemon.host().expect("host").list_sessions().is_empty());
        assert!(daemon.ownership().lock_path().exists());

        daemon.shutdown().expect("shutdown daemon");
        let _ = std::fs::remove_file(daemon.ownership().state_path());
        let _ = std::fs::remove_dir_all(daemon.ownership().runtime_root());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn daemon_save_and_load_round_trip_preserves_host_state() {
        let path = snapshot_path("muldex-daemon-roundtrip.json");
        cleanup(&path);
        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot empty daemon");
        daemon
            .host_mut()
            .expect("host")
            .create_session("session-1", sample_state("daemon-1"))
            .expect("create session");
        daemon.save().expect("save daemon snapshot");
        daemon.shutdown().expect("shutdown original daemon");

        let mut restored = RuntimeDaemon::new(&path);
        restored
            .boot_from_disk_if_present()
            .expect("boot from disk");

        let sessions = restored.host().expect("restored host").list_sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "session-1");
        assert_eq!(sessions[0].phase, RuntimePhase::Ready);

        restored.shutdown().expect("shutdown restored daemon");
        let _ = std::fs::remove_file(restored.ownership().state_path());
        let _ = std::fs::remove_dir_all(restored.ownership().runtime_root());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn daemon_shutdown_persists_and_transitions_to_stopped() {
        let path = snapshot_path("muldex-daemon-shutdown.json");
        cleanup(&path);
        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot empty daemon");
        daemon.shutdown().expect("shutdown daemon");

        assert_eq!(daemon.status(), &RuntimeDaemonStatus::Stopped);
        assert!(path.exists());
        assert!(!daemon.ownership().lock_path().exists());
        assert!(daemon.ownership().state_path().exists());

        let _ = std::fs::remove_file(daemon.ownership().state_path());
        let _ = std::fs::remove_dir_all(daemon.ownership().runtime_root());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn daemon_can_process_transport_once() {
        let path = snapshot_path("muldex-daemon-process-once.json");
        cleanup(&path);
        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot empty daemon");
        daemon
            .host_mut()
            .expect("host")
            .create_session("session-1", sample_state("daemon-transport"))
            .expect("create session");

        let command = crate::daemon_transport::DaemonCommandEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-1".to_string(),
            session_id: Some("session-1".to_string()),
            command_name: "apply_command".to_string(),
            payload_kind: "RuntimeCommand".to_string(),
            payload_json: serde_json::to_string_pretty(&crate::runtime::RuntimeCommand::Decision(
                muldex_core::protocol::ContinueDecision {
                    allow_continue: true,
                    mode: muldex_core::protocol::ContinueMode::NextTurn,
                    rationale: "advance via daemon transport".to_string(),
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
                },
            ))
            .expect("serialize command"),
            created_at_ms: 1,
        };

        daemon
            .transport()
            .write_command(&command)
            .expect("write command");
        let responses = daemon
            .process_transport_once()
            .expect("process transport once");

        assert_eq!(responses.len(), 1);
        assert!(responses[0].ok);

        daemon.shutdown().expect("shutdown daemon");
        let _ = std::fs::remove_dir_all(daemon.transport().root());
        let _ = std::fs::remove_file(daemon.ownership().state_path());
        let _ = std::fs::remove_dir_all(daemon.ownership().runtime_root());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn daemon_transport_round_trip_projects_into_client_response_view() {
        let path = snapshot_path("muldex-daemon-client-roundtrip.json");
        cleanup(&path);
        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot empty daemon");
        daemon
            .host_mut()
            .expect("host")
            .create_session("session-1", sample_state("daemon-client-roundtrip"))
            .expect("create session");

        let command = crate::daemon_transport::DaemonCommandEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-client-roundtrip".to_string(),
            session_id: Some("session-1".to_string()),
            command_name: "apply_command".to_string(),
            payload_kind: "RuntimeCommand".to_string(),
            payload_json: serde_json::to_string_pretty(&crate::runtime::RuntimeCommand::Decision(
                muldex_core::protocol::ContinueDecision {
                    allow_continue: true,
                    mode: muldex_core::protocol::ContinueMode::NextTurn,
                    rationale: "advance through client roundtrip".to_string(),
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
                },
            ))
            .expect("serialize command"),
            created_at_ms: 1,
        };

        let command_path = daemon
            .transport()
            .write_command(&command)
            .expect("write command");
        assert!(command_path.exists());

        let responses = daemon
            .process_transport_once()
            .expect("process transport once");
        assert_eq!(responses.len(), 1);

        let response = daemon
            .transport()
            .read_response("cmd-client-roundtrip")
            .expect("read response");
        let view = response_view(response);

        assert!(view.ok);
        assert_eq!(view.contract.schema_version, "client-view-v1");
        match view.payload.expect("payload") {
            ClientResponsePayloadView::Step {
                phase, cycle_index, ..
            } => {
                assert_eq!(phase, "Running");
                assert_eq!(cycle_index, 1);
            }
            _ => panic!("expected step payload view"),
        }

        daemon.shutdown().expect("shutdown daemon");
        let _ = std::fs::remove_dir_all(daemon.transport().root());
        let _ = std::fs::remove_file(daemon.ownership().state_path());
        let _ = std::fs::remove_dir_all(daemon.ownership().runtime_root());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn daemon_can_serve_bounded_loop() {
        let path = snapshot_path("muldex-daemon-serve-loop.json");
        cleanup(&path);
        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot empty daemon");
        daemon
            .host_mut()
            .expect("host")
            .create_session("session-1", sample_state("daemon-loop"))
            .expect("create session");

        let command = crate::daemon_transport::DaemonCommandEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-loop-1".to_string(),
            session_id: Some("session-1".to_string()),
            command_name: "apply_command".to_string(),
            payload_kind: "RuntimeCommand".to_string(),
            payload_json: serde_json::to_string_pretty(&crate::runtime::RuntimeCommand::Decision(
                muldex_core::protocol::ContinueDecision {
                    allow_continue: true,
                    mode: muldex_core::protocol::ContinueMode::NextTurn,
                    rationale: "advance via daemon loop".to_string(),
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
                },
            ))
            .expect("serialize command"),
            created_at_ms: 1,
        };

        daemon
            .transport()
            .write_command(&command)
            .expect("write command");
        let result = daemon.serve_loop(2).expect("serve loop");

        assert_eq!(result.iterations, 2);
        assert!(result.total_responses >= 1);

        daemon.shutdown().expect("shutdown daemon");
        let _ = std::fs::remove_dir_all(daemon.transport().root());
        let _ = std::fs::remove_file(daemon.ownership().state_path());
        let _ = std::fs::remove_dir_all(daemon.ownership().runtime_root());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn daemon_refreshes_heartbeat_during_save() {
        let path = snapshot_path("muldex-daemon-heartbeat-save.json");
        cleanup(&path);
        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot empty daemon");

        let before = daemon
            .ownership()
            .read_lock()
            .expect("read lock before save")
            .last_heartbeat_ms;
        daemon.save().expect("save daemon");
        let after = daemon
            .ownership()
            .read_lock()
            .expect("read lock after save")
            .last_heartbeat_ms;

        assert!(after >= before);

        daemon.shutdown().expect("shutdown daemon");
        let _ = std::fs::remove_dir_all(daemon.ownership().runtime_root());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn daemon_ownership_can_report_stale_status() {
        let path = snapshot_path("muldex-daemon-stale-status.json");
        cleanup(&path);
        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot empty daemon");

        let lock = daemon.ownership().read_lock().expect("read lock");
        let fresh = daemon
            .ownership()
            .classify_stale(lock.last_heartbeat_ms, 100)
            .expect("classify fresh");
        let stale = daemon
            .ownership()
            .classify_stale(lock.last_heartbeat_ms.saturating_add(500), 100)
            .expect("classify stale");

        assert!(matches!(fresh, StaleOwnershipStatus::Fresh { .. }));
        assert!(matches!(stale, StaleOwnershipStatus::Stale { .. }));

        daemon.shutdown().expect("shutdown daemon");
        let _ = std::fs::remove_dir_all(daemon.ownership().runtime_root());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn daemon_force_takeover_obeys_stale_boundary() {
        let path = snapshot_path("muldex-daemon-force-takeover.json");
        cleanup(&path);
        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot empty daemon");

        let denied = daemon
            .force_takeover(u64::MAX)
            .expect_err("fresh owner should deny takeover");
        assert!(matches!(
            denied,
            RuntimeDaemonError::Local(LocalDaemonError::TakeoverDenied)
        ));

        let lock = daemon.ownership().read_lock().expect("read lock");
        daemon
            .ownership
            .force_takeover(
                current_pid(),
                lock.last_heartbeat_ms.saturating_add(1000),
                100,
            )
            .expect("stale takeover should succeed");

        daemon.shutdown().expect("shutdown daemon");
        let _ = std::fs::remove_dir_all(daemon.ownership().runtime_root());
        let _ = std::fs::remove_file(&path);
    }
}
