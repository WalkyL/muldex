use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

use muldex_core::protocol::CompressedCycleSummary;
use muldex_core::protocol::CompressionStub;
use muldex_core::protocol::RetentionClass;
use muldex_core::protocol::RunReport;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompressedRunReportView {
    pub run_id: String,
    pub thread_id: String,
    pub rationale: String,
    pub compressed_cycle_summary: Option<CompressedCycleSummary>,
}

pub fn compress_cycle_summary_exact(
    current: &RunReport,
    previous: Option<&RunReport>,
) -> Option<CompressedCycleSummary> {
    let current_summary = current.cycle_summary.as_ref()?;

    let previous_summary = previous.and_then(|report| report.cycle_summary.as_ref());
    if let Some(previous_summary) = previous_summary {
        if previous_summary == current_summary {
            return Some(CompressedCycleSummary {
                cycle_id: current_summary.cycle_id.clone(),
                retention_class: RetentionClass::MayStubIfUnchanged,
                summary: None,
                stub: Some(CompressionStub {
                    source_id: previous_summary.cycle_id.clone(),
                    same_hash: stable_hash(previous_summary),
                    unchanged_since: previous_summary.cycle_id.clone(),
                }),
            });
        }
    }

    Some(CompressedCycleSummary {
        cycle_id: current_summary.cycle_id.clone(),
        retention_class: RetentionClass::MayStubIfUnchanged,
        summary: Some(current_summary.clone()),
        stub: None,
    })
}

pub fn compress_report_exact(
    current: &RunReport,
    previous: Option<&RunReport>,
) -> CompressedRunReportView {
    CompressedRunReportView {
        run_id: current.run_id.clone(),
        thread_id: current.thread_id.clone(),
        rationale: current.rationale.clone(),
        compressed_cycle_summary: compress_cycle_summary_exact(current, previous),
    }
}

fn stable_hash<T: serde::Serialize>(value: &T) -> String {
    let mut hasher = DefaultHasher::new();
    let encoded = serde_json::to_string(value).unwrap_or_default();
    hasher.write(encoded.as_bytes());
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use muldex_core::protocol::CycleSummary;
    use muldex_core::protocol::ExecutionMode;
    use muldex_core::protocol::RunOutcome;
    use muldex_core::protocol::StateChangeKind;

    fn report(cycle_id: &str, summary: &str) -> RunReport {
        RunReport {
            run_id: "run-1".to_string(),
            thread_id: "thread-1".to_string(),
            objective: "continue task".to_string(),
            execution_mode: ExecutionMode::Interactive,
            outcome: RunOutcome::InProgress,
            rationale: summary.to_string(),
            cycle_summary: Some(CycleSummary {
                cycle_id: cycle_id.to_string(),
                summary: summary.to_string(),
                completed_steps_delta: 0,
                state_changes: vec![StateChangeKind::NoMeaningfulChange],
                checkpoint_created: false,
                approval_request_id: None,
                pending_interrupt_count: 0,
            }),
            generated_at_ms: None,
        }
    }

    #[test]
    fn compression_keeps_full_summary_when_no_previous_report_exists() {
        let current = report("cycle-1", "initial cycle summary");
        let compressed = compress_cycle_summary_exact(&current, None).expect("compressed summary");

        assert!(compressed.summary.is_some());
        assert!(compressed.stub.is_none());
    }

    #[test]
    fn compression_stubs_unchanged_summary_when_exact_match_exists() {
        let previous = report("cycle-1", "same summary");
        let current = previous.clone();

        let compressed =
            compress_cycle_summary_exact(&current, Some(&previous)).expect("compressed summary");

        assert!(compressed.summary.is_none());
        assert!(compressed.stub.is_some());
        assert_eq!(compressed.retention_class, RetentionClass::MayStubIfUnchanged);
    }

    #[test]
    fn compression_keeps_full_summary_when_content_changes() {
        let previous = report("cycle-1", "old summary");
        let current = report("cycle-2", "new summary");

        let compressed =
            compress_cycle_summary_exact(&current, Some(&previous)).expect("compressed summary");

        assert!(compressed.summary.is_some());
        assert!(compressed.stub.is_none());
    }

    #[test]
    fn report_compression_wraps_cycle_summary_result() {
        let previous = report("cycle-1", "same summary");
        let current = previous.clone();

        let compressed = compress_report_exact(&current, Some(&previous));

        assert_eq!(compressed.run_id, "run-1");
        assert!(compressed.compressed_cycle_summary.is_some());
        assert!(compressed
            .compressed_cycle_summary
            .as_ref()
            .expect("cycle summary")
            .stub
            .is_some());
    }
}
