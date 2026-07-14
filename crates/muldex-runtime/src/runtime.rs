use serde::Deserialize;
use serde::Serialize;

use muldex_core::protocol::ContinueDecision;
use muldex_core::protocol::ContinueMode;
use muldex_core::protocol::ContinueReason;
use muldex_core::protocol::ContinueRequest;
use muldex_core::protocol::CycleSummary;
use muldex_core::protocol::InterruptInjectionMode;
use muldex_core::protocol::PendingInterrupt;
use muldex_core::protocol::PermissionDecision;
use muldex_core::protocol::PermissionDecisionStatus;
use muldex_core::protocol::RunOutcome;
use muldex_core::protocol::RunReport;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimePhase {
    Ready,
    Running,
    WaitingForApproval,
    HandedOff,
    Stopped,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeState {
    pub request: ContinueRequest,
    pub cycle_index: u32,
    pub phase: RuntimePhase,
    pub latest_report: Option<RunReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeDriver {
    pub state: RuntimeState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeEvent {
    QueueInterrupt(PendingInterrupt),
    RecordApprovalDecision(PermissionDecision),
    ReplaceContinueReason(ContinueReason),
    MarkCompleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeStepInput {
    pub decision: ContinueDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeStepResult {
    pub updated_state: RuntimeState,
    pub consumed_interrupts: Vec<PendingInterrupt>,
    pub report: RunReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeDriveResult {
    pub final_state: RuntimeState,
    pub step_results: Vec<RuntimeStepResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeResumeResult {
    pub resumed_state: RuntimeState,
    pub drive_result: RuntimeDriveResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RuntimeScriptStep {
    Event(RuntimeEvent),
    Decision(ContinueDecision),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeScriptResult {
    pub final_state: RuntimeState,
    pub event_count: usize,
    pub step_results: Vec<RuntimeStepResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RuntimeCommand {
    Event(RuntimeEvent),
    Decision(ContinueDecision),
    Drive {
        decisions: Vec<ContinueDecision>,
        cycle_limit: usize,
    },
    DriveScript {
        script: Vec<RuntimeScriptStep>,
        cycle_limit: usize,
    },
    ResumeAfterEvent {
        event: RuntimeEvent,
        decisions: Vec<ContinueDecision>,
        cycle_limit: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RuntimeCommandResult {
    EventApplied { state: RuntimeState },
    Step(RuntimeStepResult),
    Drive(RuntimeDriveResult),
    Script(RuntimeScriptResult),
    Resume(RuntimeResumeResult),
}

impl RuntimeDriver {
    pub fn new(state: RuntimeState) -> Self {
        Self { state }
    }

    pub fn ingest_event(&mut self, event: RuntimeEvent) {
        self.state = ingest_runtime_event(self.state.clone(), event);
    }

    pub fn advance(&mut self, decision: ContinueDecision) -> RuntimeStepResult {
        let result = advance_runtime(self.state.clone(), RuntimeStepInput { decision });
        self.state = result.updated_state.clone();
        result
    }

    pub fn drive(
        &mut self,
        decisions: Vec<ContinueDecision>,
        cycle_limit: usize,
    ) -> RuntimeDriveResult {
        let result = drive_runtime(self.state.clone(), decisions, cycle_limit);
        self.state = result.final_state.clone();
        result
    }

    pub fn drive_script(
        &mut self,
        script: Vec<RuntimeScriptStep>,
        cycle_limit: usize,
    ) -> RuntimeScriptResult {
        let result = drive_runtime_script(self.state.clone(), script, cycle_limit);
        self.state = result.final_state.clone();
        result
    }

    pub fn resume_after_event(
        &mut self,
        event: RuntimeEvent,
        decisions: Vec<ContinueDecision>,
        cycle_limit: usize,
    ) -> RuntimeResumeResult {
        let result = resume_runtime_after_event(self.state.clone(), event, decisions, cycle_limit);
        self.state = result.drive_result.final_state.clone();
        result
    }

    pub fn apply_command(&mut self, command: RuntimeCommand) -> RuntimeCommandResult {
        match command {
            RuntimeCommand::Event(event) => {
                self.ingest_event(event);
                RuntimeCommandResult::EventApplied {
                    state: self.state.clone(),
                }
            }
            RuntimeCommand::Decision(decision) => {
                RuntimeCommandResult::Step(self.advance(decision))
            }
            RuntimeCommand::Drive {
                decisions,
                cycle_limit,
            } => RuntimeCommandResult::Drive(self.drive(decisions, cycle_limit)),
            RuntimeCommand::DriveScript {
                script,
                cycle_limit,
            } => RuntimeCommandResult::Script(self.drive_script(script, cycle_limit)),
            RuntimeCommand::ResumeAfterEvent {
                event,
                decisions,
                cycle_limit,
            } => {
                RuntimeCommandResult::Resume(self.resume_after_event(event, decisions, cycle_limit))
            }
        }
    }
}

fn phase_from_outcome(outcome: &RunOutcome) -> RuntimePhase {
    match outcome {
        RunOutcome::InProgress | RunOutcome::Checkpointed => RuntimePhase::Running,
        RunOutcome::WaitingForApproval => RuntimePhase::WaitingForApproval,
        RunOutcome::HandedOff => RuntimePhase::HandedOff,
        RunOutcome::Stopped => RuntimePhase::Stopped,
        RunOutcome::Completed => RuntimePhase::Completed,
    }
}

pub fn ingest_runtime_event(mut state: RuntimeState, event: RuntimeEvent) -> RuntimeState {
    match event {
        RuntimeEvent::QueueInterrupt(interrupt) => {
            if interrupt.injection_mode == InterruptInjectionMode::ImmediateSafePoint {
                state.request.interrupts.safe_point_requested = true;
            }
            state.request.interrupts.last_interrupt_at_ms = interrupt.created_at_ms;
            state.request.interrupts.pending_interrupts.push(interrupt);
            if matches!(state.phase, RuntimePhase::Ready) {
                state.phase = RuntimePhase::Running;
            }
        }
        RuntimeEvent::RecordApprovalDecision(decision) => {
            let is_terminal_denial = matches!(
                decision.status,
                PermissionDecisionStatus::Denied | PermissionDecisionStatus::Expired
            );
            let decision_request_id = decision.request_id.clone();
            state.request.pending_approval.recent_decision = Some(decision);
            if state
                .request
                .pending_approval
                .active_request
                .as_ref()
                .map(|request| request.request_id.as_str())
                == Some(decision_request_id.as_str())
            {
                state.request.pending_approval.active_request = None;
            }
            state.request.pending_approval.blocked_on_approval = false;
            state.request.pending_approval.may_continue_other_work = !is_terminal_denial;
            state.phase = if is_terminal_denial {
                RuntimePhase::Stopped
            } else {
                RuntimePhase::Ready
            };
        }
        RuntimeEvent::ReplaceContinueReason(reason) => {
            state.request.continue_reason = reason;
            if matches!(state.phase, RuntimePhase::Ready) {
                state.phase = RuntimePhase::Running;
            }
        }
        RuntimeEvent::MarkCompleted => {
            state.phase = RuntimePhase::Completed;
        }
    }

    state
}

pub fn advance_runtime(state: RuntimeState, input: RuntimeStepInput) -> RuntimeStepResult {
    let mut request = state.request;
    let mut consumed_interrupts = Vec::new();

    if input.decision.consume_interrupts_now {
        let pending = std::mem::take(&mut request.interrupts.pending_interrupts);
        for interrupt in pending {
            if interrupt.injection_mode == InterruptInjectionMode::ImmediateSafePoint {
                consumed_interrupts.push(interrupt);
            } else {
                request.interrupts.pending_interrupts.push(interrupt);
            }
        }
        request.interrupts.safe_point_requested = false;
    }

    let outcome = if input.decision.pause_for_approval {
        RunOutcome::WaitingForApproval
    } else {
        match input.decision.mode {
            ContinueMode::Handoff => RunOutcome::HandedOff,
            ContinueMode::Stop => RunOutcome::Stopped,
            ContinueMode::QueueOnly | ContinueMode::NextTurn | ContinueMode::SameTurn => {
                if input.decision.request_checkpoint {
                    RunOutcome::Checkpointed
                } else {
                    RunOutcome::InProgress
                }
            }
        }
    };

    request.pending_approval.blocked_on_approval = input.decision.pause_for_approval;
    request.pending_approval.may_continue_other_work = input.decision.may_continue_other_work;

    let next_cycle_index = state.cycle_index.saturating_add(1);
    let cycle_id = format!("cycle-{}", next_cycle_index);
    let run_id = state
        .latest_report
        .as_ref()
        .map(|report| report.run_id.clone())
        .unwrap_or_else(|| format!("run:{}", request.thread_id));

    let report = RunReport {
        run_id,
        thread_id: request.thread_id.clone(),
        objective: request.objective.clone(),
        execution_mode: request
            .runtime_mode
            .active_execution_mode
            .clone()
            .unwrap_or(muldex_core::protocol::ExecutionMode::Interactive),
        outcome,
        rationale: input.decision.rationale.clone(),
        cycle_summary: Some(CycleSummary {
            cycle_id,
            summary: input.decision.rationale.clone(),
            completed_steps_delta: 0,
            state_changes: request.recent_state_changes.clone(),
            checkpoint_created: input.decision.request_checkpoint,
            approval_request_id: request
                .pending_approval
                .active_request
                .as_ref()
                .map(|request| request.request_id.clone()),
            pending_interrupt_count: request.interrupts.pending_interrupts.len(),
        }),
        generated_at_ms: None,
    };

    request.last_run_report = Some(report.clone());

    let updated_state = RuntimeState {
        request,
        cycle_index: next_cycle_index,
        phase: phase_from_outcome(&report.outcome),
        latest_report: Some(report.clone()),
    };

    RuntimeStepResult {
        updated_state,
        consumed_interrupts,
        report,
    }
}

pub fn drive_runtime(
    mut state: RuntimeState,
    decisions: Vec<ContinueDecision>,
    cycle_limit: usize,
) -> RuntimeDriveResult {
    let mut step_results = Vec::new();

    for decision in decisions.into_iter().take(cycle_limit) {
        if matches!(
            state.phase,
            RuntimePhase::WaitingForApproval
                | RuntimePhase::HandedOff
                | RuntimePhase::Stopped
                | RuntimePhase::Completed
        ) {
            break;
        }

        let result = advance_runtime(state, RuntimeStepInput { decision });
        state = result.updated_state.clone();
        let should_stop = matches!(
            state.phase,
            RuntimePhase::WaitingForApproval
                | RuntimePhase::HandedOff
                | RuntimePhase::Stopped
                | RuntimePhase::Completed
        );
        step_results.push(result);

        if should_stop {
            break;
        }
    }

    RuntimeDriveResult {
        final_state: state,
        step_results,
    }
}

pub fn resume_runtime_after_event(
    state: RuntimeState,
    event: RuntimeEvent,
    decisions: Vec<ContinueDecision>,
    cycle_limit: usize,
) -> RuntimeResumeResult {
    let resumed_state = ingest_runtime_event(state, event);
    let drive_result = drive_runtime(resumed_state.clone(), decisions, cycle_limit);

    RuntimeResumeResult {
        resumed_state,
        drive_result,
    }
}

pub fn drive_runtime_script(
    mut state: RuntimeState,
    script: Vec<RuntimeScriptStep>,
    cycle_limit: usize,
) -> RuntimeScriptResult {
    let mut event_count = 0usize;
    let mut step_results = Vec::new();
    let mut consumed_cycles = 0usize;

    for step in script {
        if matches!(
            state.phase,
            RuntimePhase::HandedOff | RuntimePhase::Stopped | RuntimePhase::Completed
        ) {
            break;
        }

        match step {
            RuntimeScriptStep::Event(event) => {
                state = ingest_runtime_event(state, event);
                event_count = event_count.saturating_add(1);
            }
            RuntimeScriptStep::Decision(decision) => {
                if consumed_cycles >= cycle_limit {
                    break;
                }
                if matches!(state.phase, RuntimePhase::WaitingForApproval) {
                    continue;
                }

                let result = advance_runtime(state, RuntimeStepInput { decision });
                state = result.updated_state.clone();
                step_results.push(result);
                consumed_cycles = consumed_cycles.saturating_add(1);
            }
        }
    }

    RuntimeScriptResult {
        final_state: state,
        event_count,
        step_results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muldex_core::protocol::ApprovalPolicyDescriptor;
    use muldex_core::protocol::ContextPressure;
    use muldex_core::protocol::ContinueReason;
    use muldex_core::protocol::ExecutionMode;
    use muldex_core::protocol::InterruptKind;
    use muldex_core::protocol::InterruptQueueState;
    use muldex_core::protocol::PendingApprovalState;
    use muldex_core::protocol::PendingInterrupt;
    use muldex_core::protocol::PermissionContextSnapshot;
    use muldex_core::protocol::PostCompactionState;
    use muldex_core::protocol::ProgressSnapshot;
    use muldex_core::protocol::RecoverySnapshot;
    use muldex_core::protocol::RuntimeModeState;
    use muldex_core::protocol::SandboxModeDescriptor;
    use muldex_core::protocol::SelfCorrectionState;
    use muldex_core::protocol::StateChangeKind;

    fn sample_state() -> RuntimeState {
        RuntimeState {
            request: ContinueRequest {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                objective: "continue task".to_string(),
                constraints: vec!["do not spin".to_string()],
                continue_reason: ContinueReason::PendingInput,
                recent_state_changes: vec![StateChangeKind::NewConfirmedFinding],
                working_hypothesis: Some("consume the next event".to_string()),
                last_agent_message: None,
                pending_input_count: 1,
                trigger_turn_pending: false,
                tool_call_count_this_turn: 0,
                context_pressure: ContextPressure::default(),
                duplicate_injection_detected: false,
                repeated_follow_up_count: 0,
                progress: ProgressSnapshot {
                    completed_steps: 1,
                    total_steps_hint: Some(2),
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
                interrupts: InterruptQueueState {
                    pending_interrupts: vec![PendingInterrupt {
                        interrupt_id: "interrupt-1".to_string(),
                        kind: InterruptKind::ApprovalDecision,
                        summary: "approval result arrived".to_string(),
                        injection_mode: InterruptInjectionMode::ImmediateSafePoint,
                        created_at_ms: Some(1),
                    }],
                    safe_point_requested: true,
                    last_interrupt_at_ms: Some(1),
                },
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
    fn runtime_consumes_immediate_safe_point_interrupts() {
        let state = sample_state();
        let result = advance_runtime(
            state,
            RuntimeStepInput {
                decision: ContinueDecision {
                    allow_continue: true,
                    mode: ContinueMode::SameTurn,
                    rationale: "consume pending safe-point interrupt".to_string(),
                    next_action: None,
                    pause_for_approval: false,
                    consume_interrupts_now: true,
                    may_continue_other_work: true,
                    suppress_duplicate_injection: false,
                    downgrade_trigger_turn: false,
                    request_compaction: false,
                    request_handoff_summary: false,
                    request_checkpoint: false,
                    enter_self_correction: false,
                },
            },
        );

        assert_eq!(result.consumed_interrupts.len(), 1);
        assert!(
            result
                .updated_state
                .request
                .interrupts
                .pending_interrupts
                .is_empty()
        );
        assert_eq!(result.report.outcome, RunOutcome::InProgress);
        assert_eq!(result.updated_state.phase, RuntimePhase::Running);
    }

    #[test]
    fn runtime_marks_waiting_for_approval_when_decision_pauses() {
        let state = sample_state();
        let result = advance_runtime(
            state,
            RuntimeStepInput {
                decision: ContinueDecision {
                    allow_continue: false,
                    mode: ContinueMode::QueueOnly,
                    rationale: "wait for approval while other work stays queued".to_string(),
                    next_action: None,
                    pause_for_approval: true,
                    consume_interrupts_now: false,
                    may_continue_other_work: true,
                    suppress_duplicate_injection: false,
                    downgrade_trigger_turn: false,
                    request_compaction: false,
                    request_handoff_summary: false,
                    request_checkpoint: false,
                    enter_self_correction: false,
                },
            },
        );

        assert_eq!(result.report.outcome, RunOutcome::WaitingForApproval);
        assert!(
            result
                .updated_state
                .request
                .pending_approval
                .blocked_on_approval
        );
        assert!(
            result
                .updated_state
                .request
                .pending_approval
                .may_continue_other_work
        );
        assert_eq!(result.updated_state.phase, RuntimePhase::WaitingForApproval);
    }

    #[test]
    fn runtime_emits_checkpointed_report_when_requested() {
        let mut state = sample_state();
        state.request.pending_approval.active_request = None;

        let result = advance_runtime(
            state,
            RuntimeStepInput {
                decision: ContinueDecision {
                    allow_continue: true,
                    mode: ContinueMode::NextTurn,
                    rationale: "checkpoint validated progress before continuing".to_string(),
                    next_action: Some("continue verification".to_string()),
                    pause_for_approval: false,
                    consume_interrupts_now: false,
                    may_continue_other_work: true,
                    suppress_duplicate_injection: false,
                    downgrade_trigger_turn: false,
                    request_compaction: false,
                    request_handoff_summary: false,
                    request_checkpoint: true,
                    enter_self_correction: false,
                },
            },
        );

        assert_eq!(result.report.outcome, RunOutcome::Checkpointed);
        assert!(
            result
                .report
                .cycle_summary
                .as_ref()
                .expect("cycle summary")
                .checkpoint_created
        );
        assert_eq!(result.updated_state.cycle_index, 1);
        assert_eq!(result.updated_state.phase, RuntimePhase::Running);
    }

    #[test]
    fn runtime_event_ingestion_can_queue_interrupts() {
        let state = sample_state();
        let updated = ingest_runtime_event(
            state,
            RuntimeEvent::QueueInterrupt(PendingInterrupt {
                interrupt_id: "interrupt-2".to_string(),
                kind: InterruptKind::UserInput,
                summary: "operator added a follow-up".to_string(),
                injection_mode: InterruptInjectionMode::ImmediateSafePoint,
                created_at_ms: Some(2),
            }),
        );

        assert_eq!(updated.request.interrupts.pending_interrupts.len(), 2);
        assert!(updated.request.interrupts.safe_point_requested);
        assert_eq!(updated.phase, RuntimePhase::Running);
    }

    #[test]
    fn approval_event_clears_waiting_state() {
        let mut state = sample_state();
        state.phase = RuntimePhase::WaitingForApproval;
        state.request.pending_approval.blocked_on_approval = true;
        state.request.pending_approval.active_request =
            Some(muldex_core::protocol::PermissionRequest {
                request_id: "approval-1".to_string(),
                action_kind: muldex_core::protocol::PermissionActionKind::RemoteMutation,
                summary: "open a pull request".to_string(),
                rationale: "share the change for review".to_string(),
                urgency: muldex_core::protocol::PermissionUrgency::Normal,
                wait_for_decision: false,
                requested_at_ms: Some(1),
                expires_at_ms: None,
            });

        let updated = ingest_runtime_event(
            state,
            RuntimeEvent::RecordApprovalDecision(PermissionDecision {
                request_id: "approval-1".to_string(),
                status: PermissionDecisionStatus::Approved,
                decided_at_ms: Some(2),
                decided_by: Some("operator".to_string()),
                note: None,
            }),
        );

        assert_eq!(updated.phase, RuntimePhase::Ready);
        assert!(!updated.request.pending_approval.blocked_on_approval);
        assert!(updated.request.pending_approval.active_request.is_none());
    }

    #[test]
    fn drive_runtime_stops_at_approval_wait_boundary() {
        let state = sample_state();
        let result = drive_runtime(
            state,
            vec![
                ContinueDecision {
                    allow_continue: true,
                    mode: ContinueMode::NextTurn,
                    rationale: "make one bounded cycle of progress".to_string(),
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
                ContinueDecision {
                    allow_continue: false,
                    mode: ContinueMode::QueueOnly,
                    rationale: "pause for approval before the risky next step".to_string(),
                    next_action: None,
                    pause_for_approval: true,
                    consume_interrupts_now: false,
                    may_continue_other_work: true,
                    suppress_duplicate_injection: false,
                    downgrade_trigger_turn: false,
                    request_compaction: false,
                    request_handoff_summary: false,
                    request_checkpoint: false,
                    enter_self_correction: false,
                },
                ContinueDecision {
                    allow_continue: true,
                    mode: ContinueMode::NextTurn,
                    rationale: "this step should never run".to_string(),
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
            ],
            8,
        );

        assert_eq!(result.step_results.len(), 2);
        assert_eq!(result.final_state.phase, RuntimePhase::WaitingForApproval);
    }

    #[test]
    fn runtime_can_resume_after_approval_event() {
        let state = sample_state();
        let waiting = drive_runtime(
            state,
            vec![ContinueDecision {
                allow_continue: false,
                mode: ContinueMode::QueueOnly,
                rationale: "pause for approval before the risky next step".to_string(),
                next_action: None,
                pause_for_approval: true,
                consume_interrupts_now: false,
                may_continue_other_work: true,
                suppress_duplicate_injection: false,
                downgrade_trigger_turn: false,
                request_compaction: false,
                request_handoff_summary: false,
                request_checkpoint: false,
                enter_self_correction: false,
            }],
            4,
        );

        let mut waiting_state = waiting.final_state;
        waiting_state.request.pending_approval.active_request =
            Some(muldex_core::protocol::PermissionRequest {
                request_id: "approval-1".to_string(),
                action_kind: muldex_core::protocol::PermissionActionKind::RemoteMutation,
                summary: "open a pull request".to_string(),
                rationale: "share the validated fix".to_string(),
                urgency: muldex_core::protocol::PermissionUrgency::Normal,
                wait_for_decision: false,
                requested_at_ms: Some(1),
                expires_at_ms: None,
            });

        let resumed = resume_runtime_after_event(
            waiting_state,
            RuntimeEvent::RecordApprovalDecision(PermissionDecision {
                request_id: "approval-1".to_string(),
                status: PermissionDecisionStatus::Approved,
                decided_at_ms: Some(2),
                decided_by: Some("operator".to_string()),
                note: Some("continue".to_string()),
            }),
            vec![ContinueDecision {
                allow_continue: true,
                mode: ContinueMode::NextTurn,
                rationale: "approval arrived, resume bounded progress".to_string(),
                next_action: Some("continue verification".to_string()),
                pause_for_approval: false,
                consume_interrupts_now: false,
                may_continue_other_work: true,
                suppress_duplicate_injection: false,
                downgrade_trigger_turn: false,
                request_compaction: false,
                request_handoff_summary: false,
                request_checkpoint: false,
                enter_self_correction: false,
            }],
            4,
        );

        assert_eq!(resumed.resumed_state.phase, RuntimePhase::Ready);
        assert_eq!(resumed.drive_result.step_results.len(), 1);
        assert_eq!(
            resumed.drive_result.final_state.phase,
            RuntimePhase::Running
        );
        assert_eq!(
            resumed
                .drive_result
                .final_state
                .latest_report
                .as_ref()
                .map(|r| &r.outcome),
            Some(&RunOutcome::InProgress)
        );
    }

    #[test]
    fn scripted_driver_can_interleave_event_and_decision_steps() {
        let state = sample_state();
        let result = drive_runtime_script(
            state,
            vec![
                RuntimeScriptStep::Decision(ContinueDecision {
                    allow_continue: false,
                    mode: ContinueMode::QueueOnly,
                    rationale: "pause for approval at the current boundary".to_string(),
                    next_action: None,
                    pause_for_approval: true,
                    consume_interrupts_now: false,
                    may_continue_other_work: true,
                    suppress_duplicate_injection: false,
                    downgrade_trigger_turn: false,
                    request_compaction: false,
                    request_handoff_summary: false,
                    request_checkpoint: false,
                    enter_self_correction: false,
                }),
                RuntimeScriptStep::Event(RuntimeEvent::RecordApprovalDecision(
                    PermissionDecision {
                        request_id: "approval-script-1".to_string(),
                        status: PermissionDecisionStatus::Approved,
                        decided_at_ms: Some(3),
                        decided_by: Some("operator".to_string()),
                        note: None,
                    },
                )),
                RuntimeScriptStep::Decision(ContinueDecision {
                    allow_continue: true,
                    mode: ContinueMode::NextTurn,
                    rationale: "approval cleared, continue work".to_string(),
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
            ],
            4,
        );

        assert_eq!(result.event_count, 1);
        assert_eq!(result.step_results.len(), 2);
        assert_eq!(result.final_state.phase, RuntimePhase::Running);
    }

    #[test]
    fn scripted_driver_stops_after_terminal_decision() {
        let state = sample_state();
        let result = drive_runtime_script(
            state,
            vec![
                RuntimeScriptStep::Decision(ContinueDecision {
                    allow_continue: false,
                    mode: ContinueMode::Stop,
                    rationale: "stop the runtime after bounded review".to_string(),
                    next_action: None,
                    pause_for_approval: false,
                    consume_interrupts_now: false,
                    may_continue_other_work: false,
                    suppress_duplicate_injection: false,
                    downgrade_trigger_turn: false,
                    request_compaction: false,
                    request_handoff_summary: false,
                    request_checkpoint: false,
                    enter_self_correction: false,
                }),
                RuntimeScriptStep::Event(RuntimeEvent::QueueInterrupt(PendingInterrupt {
                    interrupt_id: "interrupt-after-stop".to_string(),
                    kind: InterruptKind::SystemNotification,
                    summary: "this should never be processed".to_string(),
                    injection_mode: InterruptInjectionMode::QueueOnly,
                    created_at_ms: Some(5),
                })),
            ],
            4,
        );

        assert_eq!(result.step_results.len(), 1);
        assert_eq!(result.final_state.phase, RuntimePhase::Stopped);
        assert_eq!(result.event_count, 0);
    }

    #[test]
    fn runtime_driver_can_track_state_across_calls() {
        let state = sample_state();
        let mut driver = RuntimeDriver::new(state);

        let step = driver.advance(ContinueDecision {
            allow_continue: false,
            mode: ContinueMode::QueueOnly,
            rationale: "pause for approval before the next risky step".to_string(),
            next_action: None,
            pause_for_approval: true,
            consume_interrupts_now: false,
            may_continue_other_work: true,
            suppress_duplicate_injection: false,
            downgrade_trigger_turn: false,
            request_compaction: false,
            request_handoff_summary: false,
            request_checkpoint: false,
            enter_self_correction: false,
        });

        assert_eq!(step.updated_state.phase, RuntimePhase::WaitingForApproval);
        assert_eq!(driver.state.phase, RuntimePhase::WaitingForApproval);

        driver.ingest_event(RuntimeEvent::RecordApprovalDecision(PermissionDecision {
            request_id: "approval-driver-1".to_string(),
            status: PermissionDecisionStatus::Approved,
            decided_at_ms: Some(10),
            decided_by: Some("operator".to_string()),
            note: None,
        }));

        assert_eq!(driver.state.phase, RuntimePhase::Ready);
    }

    #[test]
    fn runtime_driver_can_run_scripted_flow() {
        let state = sample_state();
        let mut driver = RuntimeDriver::new(state);
        let result = driver.drive_script(
            vec![
                RuntimeScriptStep::Decision(ContinueDecision {
                    allow_continue: true,
                    mode: ContinueMode::SameTurn,
                    rationale: "consume pending interrupt first".to_string(),
                    next_action: None,
                    pause_for_approval: false,
                    consume_interrupts_now: true,
                    may_continue_other_work: true,
                    suppress_duplicate_injection: false,
                    downgrade_trigger_turn: false,
                    request_compaction: false,
                    request_handoff_summary: false,
                    request_checkpoint: false,
                    enter_self_correction: false,
                }),
                RuntimeScriptStep::Decision(ContinueDecision {
                    allow_continue: false,
                    mode: ContinueMode::Stop,
                    rationale: "finish the scripted runtime demo".to_string(),
                    next_action: None,
                    pause_for_approval: false,
                    consume_interrupts_now: false,
                    may_continue_other_work: false,
                    suppress_duplicate_injection: false,
                    downgrade_trigger_turn: false,
                    request_compaction: false,
                    request_handoff_summary: false,
                    request_checkpoint: false,
                    enter_self_correction: false,
                }),
            ],
            4,
        );

        assert_eq!(result.step_results.len(), 2);
        assert_eq!(driver.state.phase, RuntimePhase::Stopped);
        assert!(driver.state.latest_report.is_some());
    }

    #[test]
    fn runtime_driver_can_apply_normalized_commands() {
        let state = sample_state();
        let mut driver = RuntimeDriver::new(state);

        let event_result = driver.apply_command(RuntimeCommand::Event(
            RuntimeEvent::QueueInterrupt(PendingInterrupt {
                interrupt_id: "interrupt-command-1".to_string(),
                kind: InterruptKind::UserInput,
                summary: "operator follow-up queued".to_string(),
                injection_mode: InterruptInjectionMode::ImmediateSafePoint,
                created_at_ms: Some(7),
            }),
        ));

        match event_result {
            RuntimeCommandResult::EventApplied { state } => {
                assert_eq!(state.phase, RuntimePhase::Running);
                assert_eq!(state.request.interrupts.pending_interrupts.len(), 2);
            }
            _ => panic!("expected event-applied result"),
        }

        let step_result = driver.apply_command(RuntimeCommand::Decision(ContinueDecision {
            allow_continue: true,
            mode: ContinueMode::SameTurn,
            rationale: "consume the queued safe-point interrupt".to_string(),
            next_action: None,
            pause_for_approval: false,
            consume_interrupts_now: true,
            may_continue_other_work: true,
            suppress_duplicate_injection: false,
            downgrade_trigger_turn: false,
            request_compaction: false,
            request_handoff_summary: false,
            request_checkpoint: false,
            enter_self_correction: false,
        }));

        match step_result {
            RuntimeCommandResult::Step(result) => {
                assert_eq!(result.consumed_interrupts.len(), 2);
            }
            _ => panic!("expected single-step result"),
        }
    }
}
