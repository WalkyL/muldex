use crate::host::RuntimeHost;
use crate::host::RuntimeHostError;
use crate::host::RuntimeHostSnapshot;
use crate::host::RuntimeSessionSnapshot;
use crate::compression::CompressedRunReportView;
use crate::compression::compress_report_exact;
use crate::runtime::RuntimePhase;
use crate::runtime::RuntimeState;
use serde::Deserialize;
use serde::Serialize;
use muldex_core::protocol::ContinueReason;
use muldex_core::protocol::ContinueRequest;
use muldex_core::upstream_adapter::CodexBootstrapSnapshot;
use muldex_core::upstream_adapter::CodexLiveContinuationSnapshot;
use muldex_core::upstream_adapter::CodexSignalSnapshot;
use muldex_core::upstream_adapter::codex_bootstrap_snapshot_to_harness_request;
use muldex_core::upstream_adapter::codex_live_snapshot_to_harness_request;
use muldex_core::upstream_adapter::codex_snapshot_to_harness_request;

#[derive(Debug, Clone)]
pub enum ExternalRuntimeSnapshot {
    CodexSignal(CodexSignalSnapshot),
    CodexBootstrap(CodexBootstrapSnapshot),
    CodexLive(CodexLiveContinuationSnapshot),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReportExportMode {
    Raw,
    Compressed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExportedReportView {
    Raw(muldex_core::protocol::RunReport),
    Compressed(CompressedRunReportView),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportedSessionView {
    pub session_id: String,
    pub thread_id: String,
    pub cycle_index: u32,
    pub phase: RuntimePhase,
    pub report: Option<ExportedReportView>,
}

#[derive(Debug, thiserror::Error)]
pub enum ContinuityError {
    #[error("host continuity error: {0}")]
    Host(#[from] RuntimeHostError),
}

pub fn resume_host(snapshot: RuntimeHostSnapshot) -> Result<RuntimeHost, ContinuityError> {
    Ok(RuntimeHost::import_snapshot(snapshot)?)
}

pub fn export_host(host: &RuntimeHost) -> RuntimeHostSnapshot {
    host.export_snapshot()
}

pub fn export_session(
    host: &RuntimeHost,
    session_id: &str,
) -> Result<RuntimeSessionSnapshot, ContinuityError> {
    Ok(host.export_session(session_id)?)
}

pub fn export_session_view(
    host: &RuntimeHost,
    session_id: &str,
    mode: ReportExportMode,
    previous: Option<&muldex_core::protocol::RunReport>,
) -> Result<ExportedSessionView, ContinuityError> {
    let state = host.get_state(session_id)?;
    let report = match (mode, state.latest_report.as_ref()) {
        (_, None) => None,
        (ReportExportMode::Raw, Some(report)) => Some(ExportedReportView::Raw(report.clone())),
        (ReportExportMode::Compressed, Some(report)) => Some(ExportedReportView::Compressed(
            compress_report_exact(report, previous),
        )),
    };

    Ok(ExportedSessionView {
        session_id: session_id.to_string(),
        thread_id: state.request.thread_id.clone(),
        cycle_index: state.cycle_index,
        phase: state.phase.clone(),
        report,
    })
}

pub fn export_latest_report_raw(
    host: &RuntimeHost,
    session_id: &str,
) -> Result<Option<muldex_core::protocol::RunReport>, ContinuityError> {
    let state = host.get_state(session_id)?;
    Ok(state.latest_report.clone())
}

pub fn export_latest_report_compressed(
    host: &RuntimeHost,
    session_id: &str,
    previous: Option<&muldex_core::protocol::RunReport>,
) -> Result<Option<CompressedRunReportView>, ContinuityError> {
    let state = host.get_state(session_id)?;
    Ok(state
        .latest_report
        .as_ref()
        .map(|report| compress_report_exact(report, previous)))
}

pub fn import_external_snapshot_as_runtime_state(snapshot: ExternalRuntimeSnapshot) -> RuntimeState {
    let request = match snapshot {
        ExternalRuntimeSnapshot::CodexSignal(snapshot) => {
            let harness = codex_snapshot_to_harness_request(snapshot);
            continue_request_from_harness(harness, ContinueReason::ManualUserRequest)
        }
        ExternalRuntimeSnapshot::CodexBootstrap(snapshot) => {
            let harness = codex_bootstrap_snapshot_to_harness_request(snapshot);
            continue_request_from_harness(harness, ContinueReason::ManualUserRequest)
        }
        ExternalRuntimeSnapshot::CodexLive(snapshot) => {
            let harness = codex_live_snapshot_to_harness_request(snapshot);
            continue_request_from_harness(harness, ContinueReason::ManualUserRequest)
        }
    };

    RuntimeState {
        request,
        cycle_index: 0,
        phase: RuntimePhase::Ready,
        latest_report: None,
    }
}

fn continue_request_from_harness(
    harness: muldex_core::reasoning_harness::ReasoningHarnessRequest,
    continue_reason: ContinueReason,
) -> ContinueRequest {
    let thread_id = harness
        .codex_continuation
        .as_ref()
        .map(|snapshot| snapshot.source_thread_id.clone())
        .unwrap_or_else(|| "thread-imported".to_string());
    let turn_id = harness
        .codex_continuation
        .as_ref()
        .map(|snapshot| snapshot.source_turn_id.clone())
        .unwrap_or_else(|| "turn-imported".to_string());

    ContinueRequest {
        thread_id,
        turn_id,
        objective: harness.objective,
        constraints: harness.constraints,
        continue_reason,
        recent_state_changes: vec![],
        working_hypothesis: None,
        last_agent_message: None,
        pending_input_count: 0,
        trigger_turn_pending: false,
        tool_call_count_this_turn: 0,
        context_pressure: harness.context_pressure,
        duplicate_injection_detected: false,
        repeated_follow_up_count: 0,
        progress: harness.progress,
        recovery: harness.recovery,
        last_checkpoint: harness.last_checkpoint,
        self_correction: harness.self_correction,
        post_compaction: harness.post_compaction,
        runtime_mode: harness.runtime_mode,
        pending_approval: harness.pending_approval,
        interrupts: harness.interrupts,
        last_run_report: harness.last_run_report,
        safety: harness.safety,
        codex_continuation: harness.codex_continuation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::RuntimeHost;
    use muldex_core::protocol::ApprovalPolicyDescriptor;
    use muldex_core::protocol::ContextPressure;
    use muldex_core::protocol::ContinueDecision;
    use muldex_core::protocol::ContinueMode;
    use muldex_core::protocol::ContinueReason;
    use muldex_core::protocol::ContinueRequest;
    use muldex_core::protocol::ExecutionMode;
    use muldex_core::protocol::InterruptQueueState;
    use muldex_core::protocol::PendingApprovalState;
    use muldex_core::protocol::PermissionContextSnapshot;
    use muldex_core::protocol::ProgressSnapshot;
    use muldex_core::protocol::RecoverySnapshot;
    use muldex_core::protocol::RuntimeModeState;
    use muldex_core::protocol::SandboxModeDescriptor;
    use muldex_core::protocol::SelfCorrectionState;
    use muldex_core::protocol::PostCompactionState;
    use muldex_core::upstream_adapter::CodexBootstrapSnapshot;

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
                    recent_state_changes: vec![],
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
    fn continuity_can_export_and_resume_host() {
        let host = sample_host();
        let snapshot = export_host(&host);
        let resumed = resume_host(snapshot).expect("resume host");

        let sessions = resumed.list_sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "session-1");
    }

    #[test]
    fn continuity_can_export_single_session() {
        let host = sample_host();
        let snapshot = export_session(&host, "session-1").expect("export session");

        assert_eq!(snapshot.session_id, "session-1");
        assert_eq!(snapshot.driver.state.request.thread_id, "thread-1");
    }

    #[test]
    fn continuity_can_import_bootstrap_snapshot_as_runtime_state() {
        let state = import_external_snapshot_as_runtime_state(ExternalRuntimeSnapshot::CodexBootstrap(
            CodexBootstrapSnapshot {
                thread_id: "thread-42".to_string(),
                turn_id: "turn-7".to_string(),
                cwd: "/workspace".to_string(),
                model: "gpt-5.4".to_string(),
                model_provider: "llm-router".to_string(),
                collaboration_mode: "build".to_string(),
                personality: None,
                approval_policy: "OnRequest".to_string(),
                permission_profile: "managed".to_string(),
                service_tier: None,
                show_raw_agent_reasoning: false,
                model_context_window: Some(256_000),
                auto_compact_token_limit: Some(192_000),
                auto_compact_token_limit_scope: "body_after_prefix".to_string(),
                reference_context_present: true,
                prompt_input_count: 1,
                input_modalities: vec!["Text".to_string()],
                tools_visible_count: 5,
                prompt_preview_text_items: 1,
            },
        ));

        assert_eq!(state.request.thread_id, "thread-42");
        assert_eq!(state.phase, RuntimePhase::Ready);
        assert_eq!(state.request.runtime_mode.active_agent_mode.as_deref(), Some("build"));
    }

    #[test]
    fn continuity_can_export_latest_report_raw_and_compressed() {
        let mut host = sample_host();
        host.apply_command(
            "session-1",
            crate::runtime::RuntimeCommand::Decision(ContinueDecision {
                allow_continue: true,
                mode: ContinueMode::NextTurn,
                rationale: "advance once for report export".to_string(),
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
        .expect("advance host once");

        let raw = export_latest_report_raw(&host, "session-1")
            .expect("export raw")
            .expect("raw report");
        let compressed = export_latest_report_compressed(&host, "session-1", Some(&raw))
            .expect("export compressed")
            .expect("compressed report");

        assert_eq!(compressed.run_id, raw.run_id);
        assert!(compressed.compressed_cycle_summary.is_some());
        assert!(compressed
            .compressed_cycle_summary
            .as_ref()
            .expect("cycle summary")
            .stub
            .is_some());
    }

    #[test]
    fn continuity_can_export_session_view_with_compressed_report() {
        let mut host = sample_host();
        host.apply_command(
            "session-1",
            crate::runtime::RuntimeCommand::Decision(ContinueDecision {
                allow_continue: true,
                mode: ContinueMode::NextTurn,
                rationale: "advance once for session export".to_string(),
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
        .expect("advance host once");

        let previous = export_latest_report_raw(&host, "session-1")
            .expect("export previous raw")
            .expect("previous report");

        let view = export_session_view(
            &host,
            "session-1",
            ReportExportMode::Compressed,
            Some(&previous),
        )
        .expect("export session view");

        assert_eq!(view.session_id, "session-1");
        assert_eq!(view.thread_id, "thread-1");
        match view.report.expect("report view") {
            ExportedReportView::Compressed(report) => {
                assert_eq!(report.run_id, previous.run_id);
                assert!(report
                    .compressed_cycle_summary
                    .as_ref()
                    .expect("cycle summary")
                    .stub
                    .is_some());
            }
            _ => panic!("expected compressed report view"),
        }
    }
}
