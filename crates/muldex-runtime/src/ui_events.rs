use serde::Deserialize;
use serde::Serialize;

use muldex_core::provider::ProviderToolCall;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptCellKind {
    System,
    User,
    Assistant,
    Tool,
    Approval,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptCell {
    pub kind: TranscriptCellKind,
    pub title: String,
    pub content: String,
    pub pending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiEvent {
    TurnStarted { prompt: String, model: String },
    AssistantDelta { delta: String },
    AssistantMessageFinalized { content: String },
    ToolCallProposed { call: ProviderToolCall },
    ApprovalRequested { summary: String },
    ToolExecutionStarted { tool_name: String },
    ToolExecutionFinished { tool_name: String, result: String },
    TurnFailed { error: String },
    TurnCompleted,
    Usage {
        input_tokens: u64,
        cached_input_tokens: u64,
        output_tokens: u64,
        total_tokens: u64,
    },
    RateLimit {
        limit_requests: Option<u64>,
        remaining_requests: Option<u64>,
        limit_tokens: Option<u64>,
        remaining_tokens: Option<u64>,
        reset_after_seconds: Option<u64>,
    },
}

pub fn project_events_to_cells(events: &[UiEvent]) -> Vec<TranscriptCell> {
    let mut cells = Vec::new();
    let mut live_assistant: Option<TranscriptCell> = None;

    for event in events {
        match event {
            UiEvent::TurnStarted { prompt, .. } => {
                cells.push(TranscriptCell {
                    kind: TranscriptCellKind::User,
                    title: "USER".to_string(),
                    content: prompt.clone(),
                    pending: false,
                });
                live_assistant = Some(TranscriptCell {
                    kind: TranscriptCellKind::Assistant,
                    title: "ASSISTANT".to_string(),
                    content: String::new(),
                    pending: true,
                });
            }
            UiEvent::AssistantDelta { delta } => {
                if let Some(cell) = live_assistant.as_mut() {
                    cell.content.push_str(delta);
                }
            }
            UiEvent::AssistantMessageFinalized { content } => {
                let mut cell = live_assistant.take().unwrap_or(TranscriptCell {
                    kind: TranscriptCellKind::Assistant,
                    title: "ASSISTANT".to_string(),
                    content: String::new(),
                    pending: false,
                });
                if cell.content.is_empty() {
                    cell.content = content.clone();
                }
                cell.pending = false;
                cells.push(cell);
            }
            UiEvent::ToolCallProposed { call } => {
                if let Some(cell) = live_assistant.take() {
                    if !cell.content.is_empty() || cell.pending {
                        cells.push(cell);
                    }
                }
                cells.push(TranscriptCell {
                    kind: TranscriptCellKind::Tool,
                    title: format!("TOOL {}", call.name),
                    content: call.arguments_json.clone(),
                    pending: true,
                });
            }
            UiEvent::ApprovalRequested { summary } => {
                cells.push(TranscriptCell {
                    kind: TranscriptCellKind::Approval,
                    title: "APPROVAL".to_string(),
                    content: summary.clone(),
                    pending: true,
                });
            }
            UiEvent::ToolExecutionStarted { tool_name } => {
                cells.push(TranscriptCell {
                    kind: TranscriptCellKind::Tool,
                    title: format!("TOOL {tool_name}"),
                    content: "running".to_string(),
                    pending: true,
                });
            }
            UiEvent::ToolExecutionFinished { tool_name, result } => {
                cells.push(TranscriptCell {
                    kind: TranscriptCellKind::Tool,
                    title: format!("TOOL {tool_name}"),
                    content: result.clone(),
                    pending: false,
                });
            }
            UiEvent::TurnFailed { error } => {
                if let Some(cell) = live_assistant.take() {
                    if !cell.content.is_empty() {
                        cells.push(cell);
                    }
                }
                cells.push(TranscriptCell {
                    kind: TranscriptCellKind::System,
                    title: "ERROR".to_string(),
                    content: error.clone(),
                    pending: false,
                });
            }
            UiEvent::TurnCompleted => {
                if let Some(mut cell) = live_assistant.take() {
                    cell.pending = false;
                    cells.push(cell);
                }
            }
            UiEvent::Usage { .. } | UiEvent::RateLimit { .. } => {}
        }
    }

    if let Some(cell) = live_assistant {
        cells.push(cell);
    }

    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_projection_builds_live_assistant_cell_then_finalizes_it() {
        let cells = project_events_to_cells(&[
            UiEvent::TurnStarted {
                prompt: "hello".to_string(),
                model: "gpt-5".to_string(),
            },
            UiEvent::AssistantDelta {
                delta: "hi".to_string(),
            },
            UiEvent::AssistantDelta {
                delta: " there".to_string(),
            },
            UiEvent::AssistantMessageFinalized {
                content: "hi there".to_string(),
            },
            UiEvent::TurnCompleted,
        ]);

        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].kind, TranscriptCellKind::User);
        assert_eq!(cells[1].kind, TranscriptCellKind::Assistant);
        assert_eq!(cells[1].content, "hi there");
        assert!(!cells[1].pending);
    }

    #[test]
    fn event_projection_keeps_tool_and_error_cells_separate_from_assistant() {
        let cells = project_events_to_cells(&[
            UiEvent::TurnStarted {
                prompt: "status".to_string(),
                model: "gpt-5".to_string(),
            },
            UiEvent::AssistantDelta {
                delta: "checking".to_string(),
            },
            UiEvent::ToolCallProposed {
                call: ProviderToolCall {
                    id: "call_1".to_string(),
                    name: "session.status".to_string(),
                    arguments_json: "{}".to_string(),
                },
            },
            UiEvent::ToolExecutionFinished {
                tool_name: "session.status".to_string(),
                result: "ok".to_string(),
            },
            UiEvent::TurnFailed {
                error: "boom".to_string(),
            },
        ]);

        assert!(cells.iter().any(|cell| cell.title == "TOOL session.status"));
        assert!(cells.iter().any(|cell| cell.title == "ERROR"));
    }
}
