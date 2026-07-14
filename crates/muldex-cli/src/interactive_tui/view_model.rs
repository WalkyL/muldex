use super::super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShellViewModel {
    pub top_bar: TopBarViewModel,
    pub transcript_items: Vec<TranscriptItemViewModel>,
    pub status_panel: StatusPanelViewModel,
    pub composer: ComposerViewModel,
    pub slash_hints: Vec<SlashHintViewModel>,
    pub search: Option<HistorySearchViewModel>,
    pub footer: FooterViewModel,
    pub overlay: OverlayViewModel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OverlayViewModel {
    pub visible: bool,
    pub title: String,
    pub lines: Vec<String>,
    pub scroll: usize,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TopBarViewModel {
    pub product_name: String,
    pub session_summary: String,
    pub phase: String,
    pub model: String,
    pub approval_mode: String,
    pub cycle_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptItemViewModel {
    pub role_label: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct UsageViewModel {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct RateLimitViewModel {
    pub limit_requests: Option<u64>,
    pub remaining_requests: Option<u64>,
    pub limit_tokens: Option<u64>,
    pub remaining_tokens: Option<u64>,
    pub reset_after_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusPanelViewModel {
    pub phase: String,
    pub objective: String,
    pub last_outcome: String,
    pub pending_approval: bool,
    pub busy: bool,
    pub compact_count: u32,
    pub resume_count: u32,
    pub provider_summary: String,
    pub model_summary: String,
    pub usage: UsageViewModel,
    pub rate_limit: RateLimitViewModel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FooterViewModel {
    pub status_badge: String,
    pub hint: String,
    pub token_summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComposerMode {
    Insert,
    Normal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComposerViewModel {
    pub text: String,
    pub cursor_line: usize,
    pub cursor_column: usize,
    pub mode: ComposerMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SlashHintViewModel {
    pub command: String,
    pub summary: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HistorySearchViewModel {
    pub lines: Vec<String>,
}

pub(crate) struct ShellViewModelInput<'a> {
    pub session_id: &'a str,
    pub runtime: &'a RuntimeState,
    pub shell: &'a InteractiveShellState,
    pub messages: &'a [InteractiveMessage],
    pub provider_summary: &'a str,
    pub prompt_buffer: &'a InteractivePromptBuffer,
    pub completion: &'a InteractiveSlashCompletionState,
    pub history: &'a InteractiveHistoryState,
    pub search: &'a InteractiveHistorySearchState,
    pub slash_hints: &'a [InteractiveSlashHint],
    pub overlay: &'a crate::interactive_tui::overlay::OverlayState,
}

/// Current composer editing mode, derived from the global Vim state. When Vim
/// mode is disabled the composer is always in insert mode.
pub(crate) fn composer_mode() -> ComposerMode {
    match crate::vim_state().lock() {
        Ok(state) if state.enabled => {
            if state.normal {
                ComposerMode::Normal
            } else {
                ComposerMode::Insert
            }
        }
        _ => ComposerMode::Insert,
    }
}

pub(crate) fn build_shell_view_model(input: ShellViewModelInput<'_>) -> ShellViewModel {
    let cursor_prefix = &input.prompt_buffer.text[..input.prompt_buffer.cursor];
    let cursor_line = cursor_prefix.chars().filter(|ch| *ch == '\n').count();
    let cursor_column = cursor_prefix
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .chars()
        .count();
    ShellViewModel {
        top_bar: TopBarViewModel {
            product_name: "muldex".to_string(),
            session_summary: format!("session: {}", input.session_id),
            phase: format!("phase: {:?}", input.runtime.phase),
            model: format!("model: {}", runtime_model_label(input.runtime, input.shell)),
            approval_mode: format!(
                "approval: {}",
                approval_policy_label(&input.runtime.request.safety.approval_policy)
            ),
            cycle_summary: format!("cycle: {}", input.runtime.cycle_index),
        },
        transcript_items: input
            .messages
            .iter()
            .map(|message| TranscriptItemViewModel {
                role_label: transcript_role_label(&message.role).to_string(),
                content: message.content.clone(),
            })
            .collect(),
        status_panel: StatusPanelViewModel {
            phase: format!("{:?}", input.runtime.phase),
            objective: input.runtime.request.objective.clone(),
            last_outcome: input
                .runtime
                .latest_report
                .as_ref()
                .map(|report| report.rationale.clone())
                .or_else(|| {
                    input
                        .runtime
                        .request
                        .last_run_report
                        .as_ref()
                        .map(|report| report.rationale.clone())
                })
                .unwrap_or_else(|| "none".to_string()),
            pending_approval: input
                .runtime
                .request
                .pending_approval
                .active_request
                .is_some()
                || input.runtime.request.pending_approval.blocked_on_approval,
            busy: matches!(input.runtime.phase, RuntimePhase::Running),
            compact_count: input.shell.compact_count,
            resume_count: input.shell.resume_count,
            provider_summary: input.provider_summary.to_string(),
            model_summary: runtime_model_label(input.runtime, input.shell),
            usage: UsageViewModel {
                input_tokens: input.shell.usage.input_tokens,
                cached_input_tokens: input.shell.usage.cached_input_tokens,
                output_tokens: input.shell.usage.output_tokens,
                total_tokens: input.shell.usage.total_tokens,
            },
            rate_limit: RateLimitViewModel {
                limit_requests: input.shell.rate_limit.limit_requests,
                remaining_requests: input.shell.rate_limit.remaining_requests,
                limit_tokens: input.shell.rate_limit.limit_tokens,
                remaining_tokens: input.shell.rate_limit.remaining_tokens,
                reset_after_seconds: input.shell.rate_limit.reset_after_seconds,
            },
        },
        composer: ComposerViewModel {
            text: input.prompt_buffer.text.clone(),
            cursor_line,
            cursor_column,
            mode: composer_mode(),
        },
        slash_hints: if input.completion.visible && !input.slash_hints.is_empty() {
            input
                .slash_hints
                .iter()
                .map(|hint| SlashHintViewModel {
                    command: hint.command.to_string(),
                    summary: hint.summary.to_string(),
                    selected: input.completion.current_command() == Some(hint.command),
                })
                .collect()
        } else {
            Vec::new()
        },
        search: if input.search.is_active() {
            let mut lines = vec![format!("reverse search active: {}", input.search.query)];
            lines.push(format!("matches: {}", input.search.matches.len()));
            if !input.search.matches.is_empty() {
                lines.push(format!(
                    "match_index: {}/{}",
                    input.search.match_index + 1,
                    input.search.matches.len()
                ));
            }
            lines.push(match input.search.current_match(input.history) {
                Some(entry) => format!("match: {entry}"),
                None => "match: none".to_string(),
            });
            Some(HistorySearchViewModel { lines })
        } else {
            None
        },
        footer: FooterViewModel {
            status_badge: if matches!(input.runtime.phase, RuntimePhase::Running) {
                "busy".to_string()
            } else if input
                .runtime
                .request
                .pending_approval
                .blocked_on_approval
            {
                "approval".to_string()
            } else {
                "idle".to_string()
            },
            token_summary: if input.shell.usage.total_tokens > 0 {
                format!(
                    "tok {}▲/{}▼",
                    input.shell.usage.output_tokens, input.shell.usage.input_tokens
                )
            } else {
                String::new()
            },
            hint: if input.search.is_active() {
                "Ctrl+R cycle • Esc restore draft".to_string()
            } else if input.completion.visible {
                "Tab complete • Enter apply • Esc close".to_string()
            } else {
                "Enter send • Ctrl+J newline • Ctrl+R search".to_string()
            },
        },
        overlay: OverlayViewModel {
            visible: input.overlay.visible,
            title: input.overlay.title.clone(),
            lines: input.overlay.lines.clone(),
            scroll: input.overlay.scroll,
            kind: match input.overlay.kind {
                crate::interactive_tui::overlay::OverlayKind::Approval => "approval".to_string(),
                crate::interactive_tui::overlay::OverlayKind::Pager => "pager".to_string(),
            },
        },
    }
}

fn transcript_role_label(role: &InteractiveMessageRole) -> &'static str {
    match role {
        InteractiveMessageRole::System => "SYSTEM",
        InteractiveMessageRole::User => "USER",
        InteractiveMessageRole::Assistant => "ASSISTANT",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muldex_core::protocol::{
        ApprovalPolicyDescriptor, ContinueReason, ContinueRequest, ExecutionMode,
        InterruptQueueState, PendingApprovalState, PermissionActionKind, PermissionContextSnapshot,
        PermissionDecisionStatus, PermissionRequest, PermissionUrgency, PostCompactionState,
        ProgressSnapshot, RecoverySnapshot, RuntimeModeState, SandboxModeDescriptor,
        SelfCorrectionState,
    };
    use muldex_runtime::runtime::{RuntimePhase, RuntimeState};

    fn sample_runtime_state() -> RuntimeState {
        RuntimeState {
            request: ContinueRequest {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                objective: "ship tui demo".to_string(),
                constraints: vec![],
                continue_reason: ContinueReason::ManualUserRequest,
                recent_state_changes: vec![],
                working_hypothesis: None,
                last_agent_message: None,
                pending_input_count: 0,
                trigger_turn_pending: false,
                tool_call_count_this_turn: 0,
                context_pressure: Default::default(),
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
                pending_approval: PendingApprovalState {
                    active_request: Some(PermissionRequest {
                        request_id: "approval-1".to_string(),
                        action_kind: PermissionActionKind::ShellExecution,
                        summary: "need approval".to_string(),
                        rationale: "test".to_string(),
                        urgency: PermissionUrgency::Normal,
                        wait_for_decision: false,
                        requested_at_ms: None,
                        expires_at_ms: None,
                    }),
                    recent_decision: Some(muldex_core::protocol::PermissionDecision {
                        request_id: "approval-0".to_string(),
                        status: PermissionDecisionStatus::Approved,
                        decided_at_ms: None,
                        decided_by: None,
                        note: None,
                    }),
                    blocked_on_approval: true,
                    may_continue_other_work: false,
                },
                interrupts: InterruptQueueState::default(),
                last_run_report: Some(muldex_core::protocol::RunReport {
                    run_id: "run-1".to_string(),
                    thread_id: "thread-1".to_string(),
                    objective: "ship tui demo".to_string(),
                    execution_mode: ExecutionMode::Interactive,
                    outcome: muldex_core::protocol::RunOutcome::InProgress,
                    rationale: "last outcome summary".to_string(),
                    cycle_summary: None,
                    generated_at_ms: None,
                }),
                safety: PermissionContextSnapshot {
                    sandbox_mode: SandboxModeDescriptor::WorkspaceWrite,
                    approval_policy: ApprovalPolicyDescriptor::OnRequest,
                    permission_profile_summary: "managed".to_string(),
                    network_access_enabled: false,
                    requires_explicit_approval_for_next_step: true,
                },
                codex_continuation: None,
            },
            cycle_index: 7,
            phase: RuntimePhase::Running,
            latest_report: None,
        }
    }

    fn sample_shell() -> InteractiveShellState {
        InteractiveShellState {
            model: "gpt-5.4".to_string(),
            approval_mode: "on-request".to_string(),
            compact_count: 2,
            resume_count: 3,
            usage: ShellUsage::default(),
            rate_limit: ShellRateLimit::default(),
        }
    }

    fn sample_messages() -> Vec<InteractiveMessage> {
        vec![
            InteractiveMessage {
                role: InteractiveMessageRole::System,
                content: "system note".to_string(),
            },
            InteractiveMessage {
                role: InteractiveMessageRole::User,
                content: "user prompt".to_string(),
            },
            InteractiveMessage {
                role: InteractiveMessageRole::Assistant,
                content: "assistant reply".to_string(),
            },
        ]
    }

    fn sample_prompt_buffer() -> InteractivePromptBuffer {
        InteractivePromptBuffer {
            text: "/mo".to_string(),
            cursor: 3,
        }
    }

    fn sample_completion() -> InteractiveSlashCompletionState {
        InteractiveSlashCompletionState {
            seed: "/mo".to_string(),
            matches: vec!["/model"],
            index: 0,
            visible: true,
        }
    }

    fn sample_hints() -> Vec<InteractiveSlashHint> {
        vec![InteractiveSlashHint {
            command: "/model",
            summary: "show or set active model",
        }]
    }

    fn sample_input<'a>(
        runtime: &'a RuntimeState,
        shell: &'a InteractiveShellState,
        messages: &'a [InteractiveMessage],
        prompt_buffer: &'a InteractivePromptBuffer,
        completion: &'a InteractiveSlashCompletionState,
        history: &'a InteractiveHistoryState,
        search: &'a InteractiveHistorySearchState,
        slash_hints: &'a [InteractiveSlashHint],
        overlay: &'a crate::interactive_tui::overlay::OverlayState,
    ) -> ShellViewModelInput<'a> {
        ShellViewModelInput {
            session_id: "interactive-session-1234567890",
            runtime,
            shell,
            messages,
            provider_summary: "llm-router / gpt-5.4",
            prompt_buffer,
            completion,
            history,
            search,
            slash_hints,
            overlay,
        }
    }

    #[test]
    fn view_model_top_bar_includes_phase_model_approval_cycle_and_session_summary() {
        let runtime = sample_runtime_state();
        let shell = sample_shell();
        let messages = sample_messages();
        let prompt_buffer = sample_prompt_buffer();
        let completion = sample_completion();
        let history = InteractiveHistoryState::default();
        let search = InteractiveHistorySearchState::default();
        let slash_hints = sample_hints();

        let view_model = build_shell_view_model(sample_input(
            &runtime,
            &shell,
            &messages,
            &prompt_buffer,
            &completion,
            &history,
            &search,
            &slash_hints,
            &crate::interactive_tui::overlay::OverlayState::default(),
        ));

        assert_eq!(view_model.top_bar.product_name, "muldex");
        assert!(
            view_model
                .top_bar
                .session_summary
                .contains("interactive-session-1234567890")
        );
        assert!(view_model.top_bar.phase.contains("Running"));
        assert!(view_model.top_bar.model.contains("gpt-5.4"));
        assert!(view_model.top_bar.approval_mode.contains("on-request"));
        assert!(view_model.top_bar.cycle_summary.contains("7"));
    }

    #[test]
    fn view_model_transcript_maps_message_roles_to_stable_labels() {
        let runtime = sample_runtime_state();
        let shell = sample_shell();
        let messages = sample_messages();
        let prompt_buffer = sample_prompt_buffer();
        let completion = sample_completion();
        let history = InteractiveHistoryState::default();
        let search = InteractiveHistorySearchState::default();
        let slash_hints = sample_hints();

        let view_model = build_shell_view_model(sample_input(
            &runtime,
            &shell,
            &messages,
            &prompt_buffer,
            &completion,
            &history,
            &search,
            &slash_hints,
            &crate::interactive_tui::overlay::OverlayState::default(),
        ));

        let labels = view_model
            .transcript_items
            .iter()
            .map(|item| item.role_label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["SYSTEM", "USER", "ASSISTANT"]);
    }

    #[test]
    fn view_model_status_panel_exposes_objective_outcome_and_provider_fields() {
        let runtime = sample_runtime_state();
        let shell = sample_shell();
        let messages = sample_messages();
        let prompt_buffer = sample_prompt_buffer();
        let completion = sample_completion();
        let history = InteractiveHistoryState::default();
        let search = InteractiveHistorySearchState::default();
        let slash_hints = sample_hints();

        let view_model = build_shell_view_model(sample_input(
            &runtime,
            &shell,
            &messages,
            &prompt_buffer,
            &completion,
            &history,
            &search,
            &slash_hints,
            &crate::interactive_tui::overlay::OverlayState::default(),
        ));

        assert_eq!(view_model.status_panel.objective, "ship tui demo");
        assert_eq!(view_model.status_panel.last_outcome, "last outcome summary");
        assert_eq!(
            view_model.status_panel.provider_summary,
            "llm-router / gpt-5.4"
        );
        assert!(view_model.status_panel.pending_approval);
        assert_eq!(view_model.status_panel.compact_count, 2);
        assert_eq!(view_model.status_panel.resume_count, 3);
    }

    #[test]
    fn view_model_status_panel_prefers_latest_report_over_request_report() {
        let mut runtime = sample_runtime_state();
        runtime.latest_report = Some(muldex_core::protocol::RunReport {
            run_id: "run-2".to_string(),
            thread_id: "thread-1".to_string(),
            objective: "ship tui demo".to_string(),
            execution_mode: ExecutionMode::Interactive,
            outcome: muldex_core::protocol::RunOutcome::InProgress,
            rationale: "newer runtime rationale".to_string(),
            cycle_summary: None,
            generated_at_ms: None,
        });
        let shell = sample_shell();
        let messages = sample_messages();
        let prompt_buffer = sample_prompt_buffer();
        let completion = sample_completion();
        let history = InteractiveHistoryState::default();
        let search = InteractiveHistorySearchState::default();
        let slash_hints = sample_hints();

        let view_model = build_shell_view_model(sample_input(
            &runtime,
            &shell,
            &messages,
            &prompt_buffer,
            &completion,
            &history,
            &search,
            &slash_hints,
            &crate::interactive_tui::overlay::OverlayState::default(),
        ));

        assert_eq!(
            view_model.status_panel.last_outcome,
            "newer runtime rationale"
        );
    }

    #[test]
    fn view_model_status_panel_exposes_token_usage_and_rate_limit() {
        let runtime = sample_runtime_state();
        let mut shell = sample_shell();
        shell.usage = ShellUsage {
            input_tokens: 120,
            cached_input_tokens: 40,
            output_tokens: 5,
            total_tokens: 125,
        };
        shell.rate_limit = ShellRateLimit {
            limit_requests: Some(100),
            remaining_requests: Some(42),
            limit_tokens: None,
            remaining_tokens: None,
            reset_after_seconds: None,
        };
        let messages = sample_messages();
        let prompt_buffer = sample_prompt_buffer();
        let completion = sample_completion();
        let history = InteractiveHistoryState::default();
        let search = InteractiveHistorySearchState::default();
        let slash_hints = sample_hints();

        let view_model = build_shell_view_model(sample_input(
            &runtime,
            &shell,
            &messages,
            &prompt_buffer,
            &completion,
            &history,
            &search,
            &slash_hints,
            &crate::interactive_tui::overlay::OverlayState::default(),
        ));

        assert_eq!(view_model.status_panel.usage.total_tokens, 125);
        assert_eq!(view_model.status_panel.usage.input_tokens, 120);
        assert_eq!(view_model.status_panel.rate_limit.remaining_requests, Some(42));
        assert_eq!(
            view_model.footer.token_summary,
            "tok 5▲/120▼"
        );
    }

    #[test]
    fn view_model_slash_hints_visible_only_when_completion_visible_and_hints_present() {
        let runtime = sample_runtime_state();
        let shell = sample_shell();
        let messages = sample_messages();
        let prompt_buffer = sample_prompt_buffer();
        let completion = sample_completion();
        let history = InteractiveHistoryState::default();
        let search = InteractiveHistorySearchState::default();
        let slash_hints = sample_hints();

        let visible = build_shell_view_model(sample_input(
            &runtime,
            &shell,
            &messages,
            &prompt_buffer,
            &completion,
            &history,
            &search,
            &slash_hints,
            &crate::interactive_tui::overlay::OverlayState::default(),
        ));
        assert_eq!(visible.slash_hints.len(), 1);

        let hidden_completion_state = InteractiveSlashCompletionState {
            seed: "/mo".to_string(),
            matches: vec!["/model"],
            index: 0,
            visible: false,
        };
        let hidden = build_shell_view_model(sample_input(
            &runtime,
            &shell,
            &messages,
            &prompt_buffer,
            &hidden_completion_state,
            &history,
            &search,
            &slash_hints,
            &crate::interactive_tui::overlay::OverlayState::default(),
        ));
        assert!(hidden.slash_hints.is_empty());

        let empty_hints: [InteractiveSlashHint; 0] = [];
        let mut overlay_state = crate::interactive_tui::overlay::OverlayState::default();
        let empty_hints = sample_input(
            &runtime,
            &shell,
            &messages,
            &prompt_buffer,
            &completion,
            &history,
            &search,
            &empty_hints,
            &overlay_state,
        );
        let no_hints = build_shell_view_model(empty_hints);
        assert!(no_hints.slash_hints.is_empty());
    }

    #[test]
    fn view_model_search_lines_present_only_when_history_search_active() {
        let runtime = sample_runtime_state();
        let shell = sample_shell();
        let messages = sample_messages();
        let prompt_buffer = sample_prompt_buffer();
        let completion = sample_completion();
        let history = InteractiveHistoryState::from_entries(vec!["search target".to_string()]);
        let mut search = InteractiveHistorySearchState::default();
        let mut search_buffer = InteractivePromptBuffer {
            text: "se".to_string(),
            cursor: 2,
        };
        assert!(search.reverse_search(&history, &mut search_buffer));
        let slash_hints = sample_hints();

        let visible = build_shell_view_model(sample_input(
            &runtime,
            &shell,
            &messages,
            &prompt_buffer,
            &completion,
            &history,
            &search,
            &slash_hints,
            &crate::interactive_tui::overlay::OverlayState::default(),
        ));
        assert!(visible.search.is_some());
        assert!(
            visible
                .search
                .expect("search")
                .lines
                .iter()
                .any(|line| line.contains("reverse search active: se"))
        );

        let hidden = build_shell_view_model(sample_input(
            &runtime,
            &shell,
            &messages,
            &prompt_buffer,
            &completion,
            &InteractiveHistoryState::default(),
            &InteractiveHistorySearchState::default(),
            &slash_hints,
            &crate::interactive_tui::overlay::OverlayState::default(),
        ));
        assert!(hidden.search.is_none());
    }
}
