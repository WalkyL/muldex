use clap::Parser;
use clap::Subcommand;
use muldex_core::protocol::CapabilityRegistrySnapshot;
use muldex_core::protocol::CheckpointRef;
use muldex_core::protocol::ContextPressure;
use muldex_core::protocol::ContinueMode;
use muldex_core::protocol::MediaContextEnvelope;
use muldex_core::protocol::ProgressSnapshot;
use muldex_core::protocol::RecoveryReason;
use muldex_core::protocol::RecoverySnapshot;
use muldex_core::protocol::SelfCorrectionState;
use muldex_core::reasoning_harness::EscalationPolicy;
use muldex_core::reasoning_harness::ProhibitionRule;
use muldex_core::reasoning_harness::ReasoningHarnessRequest;
use muldex_core::reasoning_harness::decide_reasoning_harness;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "muldex")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    DecideSample,
    DecideFile { path: PathBuf },
}

fn sample_request() -> ReasoningHarnessRequest {
    ReasoningHarnessRequest {
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
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::DecideSample => {
            let request = sample_request();
            let decision = decide_reasoning_harness(&request);
            println!("{}", serde_json::to_string_pretty(&decision)?);
        }
        Command::DecideFile { path } => {
            let raw = fs::read_to_string(path)?;
            let request: ReasoningHarnessRequest = serde_json::from_str(&raw)?;
            let decision = decide_reasoning_harness(&request);
            println!("{}", serde_json::to_string_pretty(&decision)?);
        }
    }

    let _ = ContinueMode::NextTurn;
    Ok(())
}
