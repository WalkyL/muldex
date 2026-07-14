use serde::Deserialize;
use serde::Serialize;

use crate::client_policy::ClientAccessMode;
use crate::continuity::ExportedSessionView;
use crate::daemon::RuntimeDaemon;
use crate::daemon_local::StaleOwnershipStatus;
use crate::daemon_transport::DaemonResponseEnvelope;
use crate::host::RuntimeHost;
use crate::runtime::RuntimeCommand;
use crate::runtime::RuntimeCommandResult;
use muldex_core::protocol::ContinueDecision;
use muldex_core::protocol::ContinueMode;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReadOnlyClientCapability {
    DaemonStatus,
    SessionList,
    SessionInspect,
    SessionExport,
    ResponseRead,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientContractInfo {
    pub schema_version: String,
    pub read_only_capabilities: Vec<ReadOnlyClientCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientSessionSummaryView {
    pub session_id: String,
    pub thread_id: String,
    pub cycle_index: u32,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientSessionListView {
    pub contract: ClientContractInfo,
    pub session_count: usize,
    pub sessions: Vec<ClientSessionSummaryView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientDaemonStatusView {
    pub contract: ClientContractInfo,
    pub snapshot_path: String,
    pub daemon_status: String,
    pub session_count: usize,
    pub stale_status: Option<String>,
    pub heartbeat_age_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientCommandReceiptView {
    pub contract: ClientContractInfo,
    pub command_id: String,
    pub session_id: Option<String>,
    pub command_name: String,
    pub command_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientResponseView {
    pub contract: ClientContractInfo,
    pub command_id: String,
    pub ok: bool,
    pub payload_kind: String,
    pub payload: Option<ClientResponsePayloadView>,
    pub payload_json: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClientCommandView {
    Status,
    AdvanceSample,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientCommandEnvelopeView {
    pub contract: ClientContractInfo,
    pub session_id: String,
    pub access_mode: ClientAccessMode,
    pub command: ClientCommandView,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClientResponsePayloadView {
    Error {
        message: String,
    },
    ContractMismatch {
        expected_kind: String,
        actual_kind: String,
        raw_json: String,
    },
    EventApplied {
        phase: String,
        cycle_index: u32,
    },
    Step {
        phase: String,
        cycle_index: u32,
        outcome: String,
    },
    Drive {
        final_phase: String,
        steps: usize,
    },
    Script {
        final_phase: String,
        steps: usize,
        event_count: usize,
    },
    Resume {
        resumed_phase: String,
        final_phase: String,
        steps: usize,
    },
    Unknown {
        raw_json: String,
    },
}

pub fn default_client_contract() -> ClientContractInfo {
    ClientContractInfo {
        schema_version: "client-view-v1".to_string(),
        read_only_capabilities: vec![
            ReadOnlyClientCapability::DaemonStatus,
            ReadOnlyClientCapability::SessionList,
            ReadOnlyClientCapability::SessionInspect,
            ReadOnlyClientCapability::SessionExport,
            ReadOnlyClientCapability::ResponseRead,
        ],
    }
}

pub fn command_envelope_view(
    session_id: String,
    access_mode: ClientAccessMode,
    command: ClientCommandView,
) -> ClientCommandEnvelopeView {
    ClientCommandEnvelopeView {
        contract: default_client_contract(),
        session_id,
        access_mode,
        command,
    }
}

pub fn session_list_view(host: &RuntimeHost) -> ClientSessionListView {
    let sessions = host
        .list_sessions()
        .into_iter()
        .map(|session| ClientSessionSummaryView {
            session_id: session.session_id,
            thread_id: session.thread_id,
            cycle_index: session.cycle_index,
            phase: format!("{:?}", session.phase),
        })
        .collect::<Vec<_>>();
    ClientSessionListView {
        contract: default_client_contract(),
        session_count: sessions.len(),
        sessions,
    }
}

pub fn daemon_status_view(
    daemon: &RuntimeDaemon,
    stale_status: Option<StaleOwnershipStatus>,
) -> ClientDaemonStatusView {
    let session_count = daemon
        .host()
        .map(|host| host.list_sessions().len())
        .unwrap_or(0);
    let (stale_status_text, heartbeat_age_ms) = match stale_status {
        Some(StaleOwnershipStatus::NoLock) => (Some("no_lock".to_string()), None),
        Some(StaleOwnershipStatus::Fresh { heartbeat_age_ms }) => {
            (Some("fresh".to_string()), Some(heartbeat_age_ms))
        }
        Some(StaleOwnershipStatus::Stale { heartbeat_age_ms }) => {
            (Some("stale".to_string()), Some(heartbeat_age_ms))
        }
        None => (None, None),
    };

    ClientDaemonStatusView {
        contract: default_client_contract(),
        snapshot_path: daemon.snapshot_path().display().to_string(),
        daemon_status: format!("{:?}", daemon.status()),
        session_count,
        stale_status: stale_status_text,
        heartbeat_age_ms,
    }
}

pub fn command_receipt_view(
    command_id: String,
    session_id: Option<String>,
    command_name: String,
    command_path: String,
) -> ClientCommandReceiptView {
    ClientCommandReceiptView {
        contract: default_client_contract(),
        command_id,
        session_id,
        command_name,
        command_path,
    }
}

pub fn response_view(response: DaemonResponseEnvelope) -> ClientResponseView {
    let payload = if response.ok {
        if response.payload_kind == "RuntimeCommandResult" {
            Some(project_runtime_command_result(&response.payload_json))
        } else {
            Some(ClientResponsePayloadView::ContractMismatch {
                expected_kind: "RuntimeCommandResult".to_string(),
                actual_kind: response.payload_kind.clone(),
                raw_json: response.payload_json.clone(),
            })
        }
    } else if let Some(error) = response.error.as_ref() {
        Some(ClientResponsePayloadView::Error {
            message: error.clone(),
        })
    } else {
        None
    };
    ClientResponseView {
        contract: default_client_contract(),
        command_id: response.command_id,
        ok: response.ok,
        payload_kind: response.payload_kind,
        payload,
        payload_json: response.payload_json,
        error: response.error,
    }
}

pub fn project_runtime_command_result(payload_json: &str) -> ClientResponsePayloadView {
    match serde_json::from_str::<RuntimeCommandResult>(payload_json) {
        Ok(RuntimeCommandResult::EventApplied { state }) => {
            ClientResponsePayloadView::EventApplied {
                phase: format!("{:?}", state.phase),
                cycle_index: state.cycle_index,
            }
        }
        Ok(RuntimeCommandResult::Step(step)) => ClientResponsePayloadView::Step {
            phase: format!("{:?}", step.updated_state.phase),
            cycle_index: step.updated_state.cycle_index,
            outcome: format!("{:?}", step.report.outcome),
        },
        Ok(RuntimeCommandResult::Drive(result)) => ClientResponsePayloadView::Drive {
            final_phase: format!("{:?}", result.final_state.phase),
            steps: result.step_results.len(),
        },
        Ok(RuntimeCommandResult::Script(result)) => ClientResponsePayloadView::Script {
            final_phase: format!("{:?}", result.final_state.phase),
            steps: result.step_results.len(),
            event_count: result.event_count,
        },
        Ok(RuntimeCommandResult::Resume(result)) => ClientResponsePayloadView::Resume {
            resumed_phase: format!("{:?}", result.resumed_state.phase),
            final_phase: format!("{:?}", result.drive_result.final_state.phase),
            steps: result.drive_result.step_results.len(),
        },
        Err(_) => ClientResponsePayloadView::Unknown {
            raw_json: payload_json.to_string(),
        },
    }
}

pub fn inspect_session_view(view: ExportedSessionView) -> ExportedSessionView {
    view
}

pub fn project_client_command(view: &ClientCommandView) -> RuntimeCommand {
    match view {
        ClientCommandView::Status => RuntimeCommand::Decision(ContinueDecision {
            allow_continue: true,
            mode: ContinueMode::QueueOnly,
            rationale: "status probe through daemon transport".to_string(),
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
        ClientCommandView::AdvanceSample => RuntimeCommand::Decision(ContinueDecision {
            allow_continue: true,
            mode: ContinueMode::NextTurn,
            rationale: "advance sample through daemon transport".to_string(),
            next_action: Some("continue sample work".to_string()),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::RuntimeDaemon;
    use crate::host::RuntimeHost;
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

    fn sample_host() -> RuntimeHost {
        let mut host = RuntimeHost::new();
        host.create_session(
            "session-1",
            RuntimeState {
                request: ContinueRequest {
                    thread_id: "thread-1".to_string(),
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
            },
        )
        .expect("create session");
        host
    }

    #[test]
    fn client_session_list_view_is_serializable_shape() {
        let view = session_list_view(&sample_host());
        assert_eq!(view.contract.schema_version, "client-view-v1");
        assert_eq!(view.session_count, 1);
        assert_eq!(view.sessions[0].session_id, "session-1");
    }

    #[test]
    fn client_daemon_status_view_emits_status_text() {
        let path = std::env::temp_dir().join("muldex-client-view-daemon.json");
        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot daemon");
        let view = daemon_status_view(
            &daemon,
            Some(StaleOwnershipStatus::Fresh {
                heartbeat_age_ms: 5,
            }),
        );
        assert_eq!(view.contract.schema_version, "client-view-v1");
        assert_eq!(view.daemon_status, "Running");
        assert_eq!(view.stale_status.as_deref(), Some("fresh"));
        daemon.shutdown().ok();
        let _ = std::fs::remove_dir_all(daemon.ownership().runtime_root());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn client_response_view_projects_step_payload() {
        let payload_json = serde_json::to_string(&RuntimeCommandResult::Step(
            crate::runtime::RuntimeStepResult {
                updated_state: crate::runtime::RuntimeState {
                    request: sample_host()
                        .get_state("session-1")
                        .expect("state")
                        .request
                        .clone(),
                    cycle_index: 1,
                    phase: crate::runtime::RuntimePhase::Running,
                    latest_report: None,
                },
                consumed_interrupts: Vec::new(),
                report: muldex_core::protocol::RunReport {
                    run_id: "run-1".to_string(),
                    thread_id: "thread-1".to_string(),
                    objective: "continue task".to_string(),
                    execution_mode: ExecutionMode::Interactive,
                    outcome: muldex_core::protocol::RunOutcome::InProgress,
                    rationale: "step ok".to_string(),
                    cycle_summary: None,
                    generated_at_ms: None,
                },
            },
        ))
        .expect("serialize result");

        let view = response_view(DaemonResponseEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-1".to_string(),
            ok: true,
            payload_kind: "RuntimeCommandResult".to_string(),
            payload_json,
            error: None,
            created_at_ms: 1,
        });

        match view.payload.expect("payload") {
            ClientResponsePayloadView::Step {
                phase,
                cycle_index,
                outcome,
            } => {
                assert_eq!(phase, "Running");
                assert_eq!(cycle_index, 1);
                assert_eq!(outcome, "InProgress");
            }
            _ => panic!("expected step payload view"),
        }
    }

    #[test]
    fn client_response_view_projects_event_applied_payload() {
        let payload_json = serde_json::to_string(&RuntimeCommandResult::EventApplied {
            state: crate::runtime::RuntimeState {
                request: sample_host()
                    .get_state("session-1")
                    .expect("state")
                    .request
                    .clone(),
                cycle_index: 2,
                phase: crate::runtime::RuntimePhase::WaitingForApproval,
                latest_report: None,
            },
        })
        .expect("serialize result");

        let view = response_view(DaemonResponseEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-event".to_string(),
            ok: true,
            payload_kind: "RuntimeCommandResult".to_string(),
            payload_json,
            error: None,
            created_at_ms: 1,
        });

        match view.payload.expect("payload") {
            ClientResponsePayloadView::EventApplied { phase, cycle_index } => {
                assert_eq!(phase, "WaitingForApproval");
                assert_eq!(cycle_index, 2);
            }
            _ => panic!("expected event applied payload view"),
        }
    }

    #[test]
    fn client_response_view_projects_drive_payload() {
        let host = sample_host();
        let state = host.get_state("session-1").expect("state");
        let payload_json = serde_json::to_string(&RuntimeCommandResult::Drive(
            crate::runtime::RuntimeDriveResult {
                final_state: crate::runtime::RuntimeState {
                    request: state.request.clone(),
                    cycle_index: 3,
                    phase: crate::runtime::RuntimePhase::Completed,
                    latest_report: None,
                },
                step_results: vec![
                    crate::runtime::RuntimeStepResult {
                        updated_state: crate::runtime::RuntimeState {
                            request: state.request.clone(),
                            cycle_index: 1,
                            phase: crate::runtime::RuntimePhase::Running,
                            latest_report: None,
                        },
                        consumed_interrupts: Vec::new(),
                        report: muldex_core::protocol::RunReport {
                            run_id: "run-1".to_string(),
                            thread_id: "thread-1".to_string(),
                            objective: "continue task".to_string(),
                            execution_mode: ExecutionMode::Interactive,
                            outcome: muldex_core::protocol::RunOutcome::InProgress,
                            rationale: "step one".to_string(),
                            cycle_summary: None,
                            generated_at_ms: None,
                        },
                    },
                    crate::runtime::RuntimeStepResult {
                        updated_state: crate::runtime::RuntimeState {
                            request: state.request.clone(),
                            cycle_index: 2,
                            phase: crate::runtime::RuntimePhase::Running,
                            latest_report: None,
                        },
                        consumed_interrupts: Vec::new(),
                        report: muldex_core::protocol::RunReport {
                            run_id: "run-1".to_string(),
                            thread_id: "thread-1".to_string(),
                            objective: "continue task".to_string(),
                            execution_mode: ExecutionMode::Interactive,
                            outcome: muldex_core::protocol::RunOutcome::InProgress,
                            rationale: "step two".to_string(),
                            cycle_summary: None,
                            generated_at_ms: None,
                        },
                    },
                ],
            },
        ))
        .expect("serialize result");

        let view = response_view(DaemonResponseEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-drive".to_string(),
            ok: true,
            payload_kind: "RuntimeCommandResult".to_string(),
            payload_json,
            error: None,
            created_at_ms: 1,
        });

        match view.payload.expect("payload") {
            ClientResponsePayloadView::Drive { final_phase, steps } => {
                assert_eq!(final_phase, "Completed");
                assert_eq!(steps, 2);
            }
            _ => panic!("expected drive payload view"),
        }
    }

    #[test]
    fn client_response_view_projects_script_payload() {
        let host = sample_host();
        let state = host.get_state("session-1").expect("state");
        let payload_json = serde_json::to_string(&RuntimeCommandResult::Script(
            crate::runtime::RuntimeScriptResult {
                final_state: crate::runtime::RuntimeState {
                    request: state.request.clone(),
                    cycle_index: 4,
                    phase: crate::runtime::RuntimePhase::HandedOff,
                    latest_report: None,
                },
                event_count: 3,
                step_results: vec![crate::runtime::RuntimeStepResult {
                    updated_state: crate::runtime::RuntimeState {
                        request: state.request.clone(),
                        cycle_index: 1,
                        phase: crate::runtime::RuntimePhase::Running,
                        latest_report: None,
                    },
                    consumed_interrupts: Vec::new(),
                    report: muldex_core::protocol::RunReport {
                        run_id: "run-script".to_string(),
                        thread_id: "thread-1".to_string(),
                        objective: "continue task".to_string(),
                        execution_mode: ExecutionMode::Interactive,
                        outcome: muldex_core::protocol::RunOutcome::InProgress,
                        rationale: "script step".to_string(),
                        cycle_summary: None,
                        generated_at_ms: None,
                    },
                }],
            },
        ))
        .expect("serialize result");

        let view = response_view(DaemonResponseEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-script".to_string(),
            ok: true,
            payload_kind: "RuntimeCommandResult".to_string(),
            payload_json,
            error: None,
            created_at_ms: 1,
        });

        match view.payload.expect("payload") {
            ClientResponsePayloadView::Script {
                final_phase,
                steps,
                event_count,
            } => {
                assert_eq!(final_phase, "HandedOff");
                assert_eq!(steps, 1);
                assert_eq!(event_count, 3);
            }
            _ => panic!("expected script payload view"),
        }
    }

    #[test]
    fn client_response_view_projects_resume_payload() {
        let host = sample_host();
        let state = host.get_state("session-1").expect("state");
        let payload_json = serde_json::to_string(&RuntimeCommandResult::Resume(
            crate::runtime::RuntimeResumeResult {
                resumed_state: crate::runtime::RuntimeState {
                    request: state.request.clone(),
                    cycle_index: 1,
                    phase: crate::runtime::RuntimePhase::Running,
                    latest_report: None,
                },
                drive_result: crate::runtime::RuntimeDriveResult {
                    final_state: crate::runtime::RuntimeState {
                        request: state.request.clone(),
                        cycle_index: 2,
                        phase: crate::runtime::RuntimePhase::Stopped,
                        latest_report: None,
                    },
                    step_results: vec![crate::runtime::RuntimeStepResult {
                        updated_state: crate::runtime::RuntimeState {
                            request: state.request.clone(),
                            cycle_index: 2,
                            phase: crate::runtime::RuntimePhase::Stopped,
                            latest_report: None,
                        },
                        consumed_interrupts: Vec::new(),
                        report: muldex_core::protocol::RunReport {
                            run_id: "run-resume".to_string(),
                            thread_id: "thread-1".to_string(),
                            objective: "continue task".to_string(),
                            execution_mode: ExecutionMode::Interactive,
                            outcome: muldex_core::protocol::RunOutcome::Stopped,
                            rationale: "resume step".to_string(),
                            cycle_summary: None,
                            generated_at_ms: None,
                        },
                    }],
                },
            },
        ))
        .expect("serialize result");

        let view = response_view(DaemonResponseEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-resume".to_string(),
            ok: true,
            payload_kind: "RuntimeCommandResult".to_string(),
            payload_json,
            error: None,
            created_at_ms: 1,
        });

        match view.payload.expect("payload") {
            ClientResponsePayloadView::Resume {
                resumed_phase,
                final_phase,
                steps,
            } => {
                assert_eq!(resumed_phase, "Running");
                assert_eq!(final_phase, "Stopped");
                assert_eq!(steps, 1);
            }
            _ => panic!("expected resume payload view"),
        }
    }

    #[test]
    fn client_response_view_projects_unknown_payload_when_json_is_invalid() {
        let raw_json = "{not valid json}".to_string();
        let view = response_view(DaemonResponseEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-unknown".to_string(),
            ok: true,
            payload_kind: "RuntimeCommandResult".to_string(),
            payload_json: raw_json.clone(),
            error: None,
            created_at_ms: 1,
        });

        match view.payload.expect("payload") {
            ClientResponsePayloadView::Unknown {
                raw_json: view_json,
            } => {
                assert_eq!(view_json, raw_json);
            }
            _ => panic!("expected unknown payload view"),
        }
    }

    #[test]
    fn client_response_view_projects_error_payload_when_response_failed() {
        let error_text = "unsupported daemon command payload_kind: NotRuntimeCommand".to_string();
        let view = response_view(DaemonResponseEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-error".to_string(),
            ok: false,
            payload_kind: "Error".to_string(),
            payload_json: String::new(),
            error: Some(error_text.clone()),
            created_at_ms: 1,
        });

        match view.payload.expect("payload") {
            ClientResponsePayloadView::Error { message } => {
                assert_eq!(message, error_text);
            }
            _ => panic!("expected error payload view"),
        }
    }

    #[test]
    fn client_response_view_projects_contract_mismatch_when_payload_kind_is_wrong() {
        let raw_json = "{\"ok\":true}".to_string();
        let view = response_view(DaemonResponseEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-mismatch".to_string(),
            ok: true,
            payload_kind: "UnexpectedKind".to_string(),
            payload_json: raw_json.clone(),
            error: None,
            created_at_ms: 1,
        });

        match view.payload.expect("payload") {
            ClientResponsePayloadView::ContractMismatch {
                expected_kind,
                actual_kind,
                raw_json: mismatch_json,
            } => {
                assert_eq!(expected_kind, "RuntimeCommandResult");
                assert_eq!(actual_kind, "UnexpectedKind");
                assert_eq!(mismatch_json, raw_json);
            }
            _ => panic!("expected contract mismatch payload view"),
        }
    }

    #[test]
    fn client_command_envelope_view_carries_contract_metadata() {
        let envelope = command_envelope_view(
            "session-1".to_string(),
            ClientAccessMode::ReadOnly,
            ClientCommandView::Status,
        );
        assert_eq!(envelope.contract.schema_version, "client-view-v1");
        assert_eq!(envelope.session_id, "session-1");
    }

    #[test]
    fn client_command_receipt_view_carries_contract_metadata() {
        let receipt = command_receipt_view(
            "cmd-1".to_string(),
            Some("session-1".to_string()),
            "apply_command".to_string(),
            "D:/tmp/command.json".to_string(),
        );
        assert_eq!(receipt.contract.schema_version, "client-view-v1");
        assert_eq!(receipt.command_id, "cmd-1");
    }

    #[test]
    fn client_command_view_projects_to_runtime_command() {
        let runtime_command = project_client_command(&ClientCommandView::Status);
        match runtime_command {
            RuntimeCommand::Decision(decision) => {
                assert_eq!(decision.mode, ContinueMode::QueueOnly);
            }
            _ => panic!("expected decision command"),
        }
    }
}
