use clap::Parser;
use clap::Subcommand;
use muldex_core::protocol::CapabilityRegistrySnapshot;
use muldex_core::protocol::CheckpointRef;
use muldex_core::protocol::ContextPressure;
use muldex_core::protocol::ContinueMode;
use muldex_core::protocol::MediaAssetRef;
use muldex_core::protocol::MediaContextEnvelope;
use muldex_core::protocol::MediaKind;
use muldex_core::protocol::MediaSource;
use muldex_core::protocol::PermissionContextSnapshot;
use muldex_core::protocol::PostCompactionState;
use muldex_core::protocol::ProgressSnapshot;
use muldex_core::protocol::RecoveryReason;
use muldex_core::protocol::RecoverySnapshot;
use muldex_core::protocol::RuntimeModeState;
use muldex_core::protocol::SandboxModeDescriptor;
use muldex_core::protocol::SelfCorrectionState;
use muldex_core::protocol::SkillInvocationState;
use muldex_core::protocol::ApprovalPolicyDescriptor;
use muldex_core::protocol::CodexSessionContinuationSnapshot;
use muldex_core::reasoning_harness::EscalationPolicy;
use muldex_core::reasoning_harness::ProhibitionRule;
use muldex_core::reasoning_harness::ReasoningHarnessRequest;
use muldex_core::reasoning_harness::decide_reasoning_harness;
use muldex_core::upstream_adapter::CodexBootstrapSnapshot;
use muldex_core::upstream_adapter::CodexLiveContinuationSnapshot;
use muldex_core::upstream_adapter::CodexSignalSnapshot;
use muldex_core::upstream_adapter::codex_bootstrap_snapshot_to_harness_request;
use muldex_core::upstream_adapter::codex_live_snapshot_to_harness_request;
use muldex_core::upstream_adapter::codex_snapshot_to_harness_request;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "muldex")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum Scenario {
    Healthy,
    NoProgress,
    RecoverableFailure,
    PostCompactionStall,
    MediaHeavy,
}

#[derive(Debug, Subcommand)]
enum Command {
    DecideSample {
        #[arg(long, value_enum, default_value = "healthy")]
        scenario: Scenario,
    },
    DecideFile { path: PathBuf },
    DecideCodexSnapshot { path: PathBuf },
    DecideWorkspace {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        objective: Option<String>,
        #[arg(long = "objective-file")]
        objective_file: Option<PathBuf>,
        #[arg(long, default_value = "build")]
        mode: String,
        #[arg(long, default_value_t = 0)]
        no_progress_iterations: u32,
        #[arg(long, default_value_t = false)]
        post_compaction: bool,
        #[arg(long, default_value_t = false)]
        recoverable_failure: bool,
        #[arg(long, default_value_t = false)]
        print_request: bool,
    },
}

fn sample_request(scenario: Scenario) -> ReasoningHarnessRequest {
    let mut request = ReasoningHarnessRequest {
        objective: "continue a long-running coding task".to_string(),
        constraints: vec![
            "do not spin".to_string(),
            "checkpoint before handoff".to_string(),
        ],
        evidence_scope: vec!["current repository".to_string()],
        allowed_capability_classes: vec!["tool".to_string(), "skill".to_string()],
        prohibited_behaviors: vec![
            ProhibitionRule::NoFakeProgress,
            ProhibitionRule::NoRepeatedNoProgressContinuation,
            ProhibitionRule::NoDuplicateInjection,
        ],
        progress: ProgressSnapshot {
            completed_steps: 3,
            total_steps_hint: Some(8),
            last_meaningful_progress_at_ms: Some(1_700_000_000_000),
            no_progress_iteration_count: 1,
        },
        recovery: RecoverySnapshot {
            last_recovery_reason: Some(RecoveryReason::PartialResult),
            recovery_attempt_count: 1,
            last_recovery_had_progress: true,
        },
        last_checkpoint: Some(CheckpointRef {
            checkpoint_id: "cp-1".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-5".to_string(),
            created_at_ms: 1_700_000_000_000,
            summary: "checkpoint after media analysis".to_string(),
        }),
        self_correction: SelfCorrectionState {
            active: false,
            correction_attempt_count: 0,
            last_correction_target: None,
            last_correction_had_progress: false,
        },
        post_compaction: PostCompactionState {
            pending_post_compaction: false,
            first_post_compaction_turn: false,
            compaction_window_id: Some("window-1".to_string()),
            last_compaction_checkpoint_id: Some("cp-1".to_string()),
        },
        runtime_mode: RuntimeModeState {
            active_agent_mode: Some("build".to_string()),
            previous_agent_mode: Some("plan".to_string()),
            mode_transition_pending_guidance: false,
            invoked_skills: vec![SkillInvocationState {
                skill_id: "context-budget-gate".to_string(),
                invocation_ref: Some("skill://gate/1".to_string()),
                invoked_at_ms: Some(1_700_000_000_123),
            }],
        },
        safety: PermissionContextSnapshot {
            sandbox_mode: SandboxModeDescriptor::WorkspaceWrite,
            approval_policy: ApprovalPolicyDescriptor::OnRequest,
            permission_profile_summary: "managed".to_string(),
            network_access_enabled: false,
            requires_explicit_approval_for_next_step: false,
        },
        codex_continuation: Some(CodexSessionContinuationSnapshot {
            source_thread_id: "thread-1".to_string(),
            source_turn_id: "turn-5".to_string(),
            source_model: "gpt-5.4".to_string(),
            source_provider: "llm-router".to_string(),
            active_agent_mode: Some("build".to_string()),
            safety: PermissionContextSnapshot {
                sandbox_mode: SandboxModeDescriptor::WorkspaceWrite,
                approval_policy: ApprovalPolicyDescriptor::OnRequest,
                permission_profile_summary: "managed".to_string(),
                network_access_enabled: false,
                requires_explicit_approval_for_next_step: false,
            },
            reference_context_present: true,
        }),
        context_pressure: ContextPressure {
            model_context_window: Some(256_000),
            active_context_tokens: Some(140_000),
            auto_compact_scope_tokens: Some(24_000),
            auto_compact_limit: Some(192_000),
            tokens_until_compaction: Some(52_000),
            recent_compaction_count: 1,
            last_compaction_had_state_change: true,
        },
        media_context: Vec::<MediaContextEnvelope>::new(),
        capability_registry: CapabilityRegistrySnapshot::default(),
        escalation_policy: EscalationPolicy {
            no_progress_limit: 3,
            repeated_compaction_limit: 2,
            self_correction_limit: 2,
            request_checkpoint_before_handoff: true,
        },
    };

    match scenario {
        Scenario::Healthy => {}
        Scenario::NoProgress => {
            request.progress.no_progress_iteration_count = 3;
            request.recovery.last_recovery_had_progress = false;
        }
        Scenario::RecoverableFailure => {
            request.recovery.last_recovery_reason = Some(RecoveryReason::ToolFailure);
            request.recovery.last_recovery_had_progress = false;
            request.self_correction.active = true;
            request.self_correction.correction_attempt_count = 1;
            request.self_correction.last_correction_target = Some("retry failed tool step".to_string());
        }
        Scenario::PostCompactionStall => {
            request.post_compaction.pending_post_compaction = true;
            request.post_compaction.first_post_compaction_turn = true;
            request.progress.no_progress_iteration_count = 2;
            request.recovery.last_recovery_had_progress = false;
        }
        Scenario::MediaHeavy => {
            request.objective = "analyze multimodal evidence and continue safely".to_string();
            request.media_context.push(MediaContextEnvelope {
                asset: crate_media_asset("video-1", "clips/demo.mp4"),
                derived_artifacts: Vec::new(),
                hyperframes: Vec::new(),
                operator_summary: "video and transcript are available".to_string(),
                model_summary: "use hyperframe-aligned evidence".to_string(),
                token_budget_hint: Some(4096),
            });
        }
    }

    request
}

fn crate_media_asset(asset_id: &str, path: &str) -> MediaAssetRef {
    MediaAssetRef {
        asset_id: asset_id.to_string(),
        kind: MediaKind::Video,
        source: MediaSource::LocalPath {
            path: path.to_string(),
        },
        display_name: Some(asset_id.to_string()),
    }
}

fn print_decision(decision: &muldex_core::reasoning_harness::ReasoningHarnessDecision) {
    println!("mode: {}", match decision.mode {
        ContinueMode::SameTurn => "same_turn",
        ContinueMode::NextTurn => "next_turn",
        ContinueMode::QueueOnly => "queue_only",
        ContinueMode::Handoff => "handoff",
        ContinueMode::Stop => "stop",
    });
    println!("checkpoint: {}", decision.should_checkpoint);
    println!("self_correction: {}", decision.should_enter_self_correction);
    println!("rationale: {}", decision.rationale);
    if !decision.violated_rules.is_empty() {
        println!("violated_rules: {:?}", decision.violated_rules);
    }
}

fn print_bootstrap_snapshot_summary(snapshot: &CodexBootstrapSnapshot) {
    println!("snapshot.kind: codex-bootstrap");
    println!("snapshot.model: {}", snapshot.model);
    println!("snapshot.provider: {}", snapshot.model_provider);
    println!("snapshot.mode: {}", snapshot.collaboration_mode);
    println!("snapshot.personality: {:?}", snapshot.personality);
    println!("snapshot.approval_policy: {}", snapshot.approval_policy);
    println!("snapshot.service_tier: {:?}", snapshot.service_tier);
    println!("snapshot.show_raw_agent_reasoning: {}", snapshot.show_raw_agent_reasoning);
    println!("snapshot.reference_context: {}", snapshot.reference_context_present);
    println!("snapshot.input_modalities: {:?}", snapshot.input_modalities);
    println!("snapshot.tools_visible: {}", snapshot.tools_visible_count);
}

fn print_live_snapshot_summary(snapshot: &CodexLiveContinuationSnapshot) {
    println!("snapshot.kind: codex-live");
    println!("snapshot.thread_id: {}", snapshot.thread_id);
    println!("snapshot.active_turn_present: {}", snapshot.active_turn_present);
    println!("snapshot.pending_input_present: {}", snapshot.pending_input_present);
    println!(
        "snapshot.trigger_turn_mailbox_present: {}",
        snapshot.trigger_turn_mailbox_present
    );
    println!(
        "snapshot.auto_compact_window_number: {}",
        snapshot.auto_compact_window_number
    );
    println!("snapshot.total_input_tokens: {:?}", snapshot.total_input_tokens);
}

fn build_workspace_request(
    workspace: PathBuf,
    objective: Option<String>,
    objective_file: Option<PathBuf>,
    mode: String,
    no_progress_iterations: u32,
    post_compaction: bool,
    recoverable_failure: bool,
) -> Result<ReasoningHarnessRequest, Box<dyn std::error::Error>> {
    if !workspace.exists() || !workspace.is_dir() {
        return Err(format!("workspace does not exist or is not a directory: {}", workspace.display()).into());
    }

    let objective = match (objective, objective_file) {
        (Some(text), None) => text,
        (None, Some(path)) => fs::read_to_string(path)?,
        (Some(_), Some(_)) => {
            return Err("provide either --objective or --objective-file, not both".into())
        }
        (None, None) => return Err("provide --objective or --objective-file".into()),
    };

    let git_hint = if workspace.join(".git").exists() {
        "git repository"
    } else {
        "non-git workspace"
    };

    let mut request = sample_request(Scenario::Healthy);
    request.objective = objective.trim().to_string();
    request.evidence_scope = vec![
        format!("workspace: {}", workspace.display()),
        format!("workspace_kind: {git_hint}"),
    ];
    request.runtime_mode.active_agent_mode = Some(mode);
    request.progress.no_progress_iteration_count = no_progress_iterations;

    if post_compaction {
        request.post_compaction.pending_post_compaction = true;
        request.post_compaction.first_post_compaction_turn = true;
    }

    if recoverable_failure {
        request.recovery.last_recovery_reason = Some(RecoveryReason::ToolFailure);
        request.recovery.last_recovery_had_progress = false;
        request.self_correction.active = true;
        request.self_correction.correction_attempt_count = 1;
        request.self_correction.last_correction_target = Some(
            "retry failed step in real workspace".to_string(),
        );
    }

    Ok(request)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::DecideSample { scenario } => {
            let request = sample_request(scenario);
            let decision = decide_reasoning_harness(&request);
            print_decision(&decision);
            println!();
            println!("{}", serde_json::to_string_pretty(&decision)?);
        }
        Command::DecideFile { path } => {
            let raw = fs::read_to_string(path)?;
            let request: ReasoningHarnessRequest = serde_json::from_str(&raw)?;
            let decision = decide_reasoning_harness(&request);
            print_decision(&decision);
            println!();
            println!("{}", serde_json::to_string_pretty(&decision)?);
        }
        Command::DecideCodexSnapshot { path } => {
            let raw = fs::read_to_string(path)?;
            let request = match serde_json::from_str::<CodexSignalSnapshot>(&raw) {
                Ok(snapshot) => codex_snapshot_to_harness_request(snapshot),
                Err(_) => match serde_json::from_str::<CodexLiveContinuationSnapshot>(&raw) {
                    Ok(live) => {
                        print_live_snapshot_summary(&live);
                        println!();
                        codex_live_snapshot_to_harness_request(live)
                    }
                    Err(_) => {
                        let bootstrap: CodexBootstrapSnapshot = serde_json::from_str(&raw)?;
                        print_bootstrap_snapshot_summary(&bootstrap);
                        println!();
                        codex_bootstrap_snapshot_to_harness_request(bootstrap)
                    }
                }
            };
            let decision = decide_reasoning_harness(&request);
            print_decision(&decision);
            println!();
            println!("{}", serde_json::to_string_pretty(&decision)?);
        }
        Command::DecideWorkspace {
            workspace,
            objective,
            objective_file,
            mode,
            no_progress_iterations,
            post_compaction,
            recoverable_failure,
            print_request,
        } => {
            let request = build_workspace_request(
                workspace,
                objective,
                objective_file,
                mode,
                no_progress_iterations,
                post_compaction,
                recoverable_failure,
            )?;
            if print_request {
                println!("{}", serde_json::to_string_pretty(&request)?);
                println!();
            }
            let decision = decide_reasoning_harness(&request);
            print_decision(&decision);
            println!();
            println!("{}", serde_json::to_string_pretty(&decision)?);
        }
    }

    let _ = ContinueMode::NextTurn;
    Ok(())
}
