use serde::Deserialize;
use serde::Serialize;

use muldex_core::provider::InteractiveProvider;
use muldex_core::provider::ProviderAssistantTurn;
use muldex_core::provider::ProviderError;
use muldex_core::provider::ProviderEventSink;
use muldex_core::provider::ProviderMessageRole;
use muldex_core::provider::ProviderStreamEvent;
use muldex_core::provider::ProviderToolCall;
use muldex_core::provider::ProviderToolSpec;
use muldex_core::provider::ProviderTurnMessage;
use muldex_core::provider::ProviderTurnRequest;
use muldex_core::provider::ResolvedProviderConfig;

use crate::react_loop::append_tool_result_message;
use crate::react_loop::assistant_turn_requires_tool_loop;
use crate::ui_events::UiEvent;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InteractiveTurnStatus {
    Idle,
    Running,
    AwaitingApproval,
    Failed,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractiveTurnState {
    pub status: InteractiveTurnStatus,
    pub last_error: Option<String>,
    pub event_count: usize,
}

impl Default for InteractiveTurnState {
    fn default() -> Self {
        Self {
            status: InteractiveTurnStatus::Idle,
            last_error: None,
            event_count: 0,
        }
    }
}

impl InteractiveTurnState {
    pub fn apply_event(&mut self, event: &UiEvent) {
        self.event_count = self.event_count.saturating_add(1);
        match event {
            UiEvent::TurnStarted { .. } => {
                self.status = InteractiveTurnStatus::Running;
                self.last_error = None;
            }
            UiEvent::ApprovalRequested { .. } => {
                self.status = InteractiveTurnStatus::AwaitingApproval;
            }
            UiEvent::TurnFailed { error } => {
                self.status = InteractiveTurnStatus::Failed;
                self.last_error = Some(error.clone());
            }
            UiEvent::TurnCompleted => {
                self.status = InteractiveTurnStatus::Completed;
            }
            UiEvent::AssistantDelta { .. }
            | UiEvent::AssistantMessageFinalized { .. }
            | UiEvent::ToolCallProposed { .. }
            | UiEvent::ToolExecutionStarted { .. }
            | UiEvent::ToolExecutionFinished { .. }
            | UiEvent::Usage { .. }
            | UiEvent::RateLimit { .. } => {
                if !matches!(self.status, InteractiveTurnStatus::AwaitingApproval) {
                    self.status = InteractiveTurnStatus::Running;
                }
            }
        }
    }
}

pub trait UiEventListener {
    fn on_event(&mut self, event: UiEvent);
}

pub trait InteractiveToolExecutor {
    fn tool_specs(&self) -> Vec<ProviderToolSpec>;
    fn execute(&mut self, call: &ProviderToolCall) -> Result<String, InteractiveToolError>;
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InteractiveToolError {
    #[error("approval required: {0}")]
    ApprovalRequired(String),
    #[error("unsupported tool: {0}")]
    Unsupported(String),
    #[error("tool failed: {0}")]
    Failed(String),
}

pub struct TurnExecutionRequest {
    pub prompt: String,
    pub model: String,
    pub prior_messages: Vec<ProviderTurnMessage>,
    pub enable_tools: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnExecutionResult {
    pub assistant: ProviderAssistantTurn,
    pub final_messages: Vec<ProviderTurnMessage>,
    pub state: InteractiveTurnState,
}

pub async fn execute_interactive_turn(
    provider: &dyn InteractiveProvider,
    provider_config: &ResolvedProviderConfig,
    request: TurnExecutionRequest,
    tool_executor: &mut dyn InteractiveToolExecutor,
    listener: &mut dyn UiEventListener,
) -> Result<TurnExecutionResult, ProviderError> {
    let mut state = InteractiveTurnState::default();
    let mut messages = request.prior_messages;
    messages.push(ProviderTurnMessage {
        role: ProviderMessageRole::User,
        content: Some(request.prompt.clone()),
        tool_call_id: None,
        name: None,
        tool_calls: Vec::new(),
    });

    let started = UiEvent::TurnStarted {
        prompt: request.prompt.clone(),
        model: request.model.clone(),
    };
    state.apply_event(&started);
    listener.on_event(started);

    let mut last_turn = ProviderAssistantTurn {
        content: String::new(),
        tool_calls: Vec::new(),
    };

    for _ in 0..4 {
        let tools = if request.enable_tools {
            tool_executor.tool_specs()
        } else {
            Vec::new()
        };
        let provider_request = ProviderTurnRequest {
            model: request.model.clone(),
            messages: messages.clone(),
            stream: true,
            tools,
        };
        let mut sink = UiEventForwarder {
            listener,
            state: &mut state,
        };
        let turn = provider
            .run_turn(provider_config, provider_request, &mut sink)
            .await?;
        last_turn = turn.clone();

        let finalized = UiEvent::AssistantMessageFinalized {
            content: turn.content.clone(),
        };
        state.apply_event(&finalized);
        listener.on_event(finalized);

        if !assistant_turn_requires_tool_loop(&turn) {
            let completed = UiEvent::TurnCompleted;
            state.apply_event(&completed);
            listener.on_event(completed);
            messages.push(ProviderTurnMessage {
                role: ProviderMessageRole::Assistant,
                content: Some(turn.content.clone()),
                tool_call_id: None,
                name: None,
                tool_calls: Vec::new(),
            });
            return Ok(TurnExecutionResult {
                assistant: turn,
                final_messages: messages,
                state,
            });
        }

        messages.push(ProviderTurnMessage {
            role: ProviderMessageRole::Assistant,
            content: if turn.content.is_empty() {
                None
            } else {
                Some(turn.content.clone())
            },
            tool_call_id: None,
            name: None,
            tool_calls: turn.tool_calls.clone(),
        });

        for call in &turn.tool_calls {
            let proposed = UiEvent::ToolCallProposed { call: call.clone() };
            state.apply_event(&proposed);
            listener.on_event(proposed);

            let started = UiEvent::ToolExecutionStarted {
                tool_name: call.name.clone(),
            };
            state.apply_event(&started);
            listener.on_event(started);

            match tool_executor.execute(call) {
                Ok(result) => {
                    let finished = UiEvent::ToolExecutionFinished {
                        tool_name: call.name.clone(),
                        result: result.clone(),
                    };
                    state.apply_event(&finished);
                    listener.on_event(finished);
                    append_tool_result_message(&mut messages, &call.name, &call.id, result);
                }
                Err(InteractiveToolError::ApprovalRequired(summary)) => {
                    let approval = UiEvent::ApprovalRequested { summary };
                    state.apply_event(&approval);
                    listener.on_event(approval);
                    return Ok(TurnExecutionResult {
                        assistant: turn,
                        final_messages: messages,
                        state,
                    });
                }
                Err(error) => {
                    let failed = UiEvent::TurnFailed {
                        error: error.to_string(),
                    };
                    state.apply_event(&failed);
                    listener.on_event(failed);
                    return Ok(TurnExecutionResult {
                        assistant: turn,
                        final_messages: messages,
                        state,
                    });
                }
            }
        }
    }

    let failed = UiEvent::TurnFailed {
        error: "tool loop exceeded max iterations".to_string(),
    };
    state.apply_event(&failed);
    listener.on_event(failed);
    Ok(TurnExecutionResult {
        assistant: last_turn,
        final_messages: messages,
        state,
    })
}

struct UiEventForwarder<'a> {
    listener: &'a mut dyn UiEventListener,
    state: &'a mut InteractiveTurnState,
}

impl ProviderEventSink for UiEventForwarder<'_> {
    fn push(&mut self, event: ProviderStreamEvent) {
        match event {
            ProviderStreamEvent::AssistantDelta(delta) => {
                let ui_event = UiEvent::AssistantDelta { delta };
                self.state.apply_event(&ui_event);
                self.listener.on_event(ui_event);
            }
            ProviderStreamEvent::Usage(usage) => {
                self.listener.on_event(UiEvent::Usage {
                    input_tokens: usage.input_tokens,
                    cached_input_tokens: usage.cached_input_tokens,
                    output_tokens: usage.output_tokens,
                    total_tokens: usage.total_tokens,
                });
            }
            ProviderStreamEvent::RateLimit(rate_limit) => {
                self.listener.on_event(UiEvent::RateLimit {
                    limit_requests: rate_limit.limit_requests,
                    remaining_requests: rate_limit.remaining_requests,
                    limit_tokens: rate_limit.limit_tokens,
                    remaining_tokens: rate_limit.remaining_tokens,
                    reset_after_seconds: rate_limit.reset_after_seconds,
                });
            }
            ProviderStreamEvent::ToolCallDelta(_) | ProviderStreamEvent::MessageComplete => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_events::UiEvent;
    use async_trait::async_trait;
    use muldex_core::provider::InteractiveProvider;
    use muldex_core::provider::ProviderAssistantTurn;
    use muldex_core::provider::ProviderError;
    use muldex_core::provider::ProviderStreamEvent;
    use muldex_core::provider::ProviderToolCall;
    use muldex_core::provider::ProviderToolSpec;
    use muldex_core::provider::ProviderTurnRequest;
    use muldex_core::provider::ResolvedProviderConfig;

    #[test]
    fn lifecycle_moves_idle_running_completed() {
        let mut state = InteractiveTurnState::default();
        assert_eq!(state.status, InteractiveTurnStatus::Idle);

        state.apply_event(&UiEvent::TurnStarted {
            prompt: "hello".to_string(),
            model: "gpt-5".to_string(),
        });
        assert_eq!(state.status, InteractiveTurnStatus::Running);

        state.apply_event(&UiEvent::TurnCompleted);
        assert_eq!(state.status, InteractiveTurnStatus::Completed);
    }

    #[test]
    fn lifecycle_tracks_approval_and_failure() {
        let mut state = InteractiveTurnState::default();
        state.apply_event(&UiEvent::TurnStarted {
            prompt: "hello".to_string(),
            model: "gpt-5".to_string(),
        });
        state.apply_event(&UiEvent::ApprovalRequested {
            summary: "need approval".to_string(),
        });
        assert_eq!(state.status, InteractiveTurnStatus::AwaitingApproval);

        state.apply_event(&UiEvent::TurnFailed {
            error: "network error".to_string(),
        });
        assert_eq!(state.status, InteractiveTurnStatus::Failed);
        assert_eq!(state.last_error.as_deref(), Some("network error"));
    }

    #[derive(Default)]
    struct RecordingListener {
        events: Vec<UiEvent>,
    }

    impl UiEventListener for RecordingListener {
        fn on_event(&mut self, event: UiEvent) {
            self.events.push(event);
        }
    }

    struct FakeToolExecutor {
        calls: Vec<String>,
    }

    impl InteractiveToolExecutor for FakeToolExecutor {
        fn tool_specs(&self) -> Vec<ProviderToolSpec> {
            vec![ProviderToolSpec {
                name: "session.status".to_string(),
                description: "status".to_string(),
                input_schema: serde_json::json!({"type":"object","properties":{}}),
            }]
        }

        fn execute(&mut self, call: &ProviderToolCall) -> Result<String, InteractiveToolError> {
            self.calls.push(call.name.clone());
            Ok("{\"phase\":\"Running\"}".to_string())
        }
    }

    struct FakeProvider {
        turns: std::sync::Mutex<Vec<ProviderAssistantTurn>>,
        request_count: std::sync::Mutex<usize>,
    }

    #[async_trait(?Send)]
    impl InteractiveProvider for FakeProvider {
        async fn run_turn(
            &self,
            _provider: &ResolvedProviderConfig,
            _request: ProviderTurnRequest,
            sink: &mut dyn ProviderEventSink,
        ) -> Result<ProviderAssistantTurn, ProviderError> {
            let mut count = self.request_count.lock().expect("request_count");
            *count += 1;
            let turns = self.turns.lock().expect("turns");
            let turn = turns.get(*count - 1).cloned().expect("fake turn present");
            if !turn.content.is_empty() {
                sink.push(ProviderStreamEvent::AssistantDelta(turn.content.clone()));
            }
            Ok(turn)
        }
    }

    #[tokio::test]
    async fn execute_interactive_turn_completes_non_tool_turn() {
        let provider = FakeProvider {
            turns: std::sync::Mutex::new(vec![ProviderAssistantTurn {
                content: "hello back".to_string(),
                tool_calls: Vec::new(),
            }]),
            request_count: std::sync::Mutex::new(0),
        };
        let mut tools = FakeToolExecutor { calls: Vec::new() };
        let mut listener = RecordingListener::default();

        let result = execute_interactive_turn(
            &provider,
            &ResolvedProviderConfig {
                name: "llm-router".to_string(),
                kind: "openai-compatible".to_string(),
                base_url: "http://127.0.0.1:3000/v1".to_string(),
                api_key: None,
                default_model: Some("gpt-5".to_string()),
            },
            TurnExecutionRequest {
                prompt: "hello".to_string(),
                model: "gpt-5".to_string(),
                prior_messages: Vec::new(),
                enable_tools: false,
            },
            &mut tools,
            &mut listener,
        )
        .await
        .expect("turn result");

        assert_eq!(result.assistant.content, "hello back");
        assert!(tools.calls.is_empty());
        assert!(listener.events.iter().any(|event| matches!(
            event,
            UiEvent::AssistantDelta { delta } if delta == "hello back"
        )));
        assert_eq!(result.state.status, InteractiveTurnStatus::Completed);
    }

    #[tokio::test]
    async fn execute_interactive_turn_runs_minimal_tool_loop() {
        let provider = FakeProvider {
            turns: std::sync::Mutex::new(vec![
                ProviderAssistantTurn {
                    content: String::new(),
                    tool_calls: vec![ProviderToolCall {
                        id: "call_1".to_string(),
                        name: "session.status".to_string(),
                        arguments_json: "{}".to_string(),
                    }],
                },
                ProviderAssistantTurn {
                    content: "status complete".to_string(),
                    tool_calls: Vec::new(),
                },
            ]),
            request_count: std::sync::Mutex::new(0),
        };
        let mut tools = FakeToolExecutor { calls: Vec::new() };
        let mut listener = RecordingListener::default();

        let result = execute_interactive_turn(
            &provider,
            &ResolvedProviderConfig {
                name: "llm-router".to_string(),
                kind: "openai-compatible".to_string(),
                base_url: "http://127.0.0.1:3000/v1".to_string(),
                api_key: None,
                default_model: Some("gpt-5".to_string()),
            },
            TurnExecutionRequest {
                prompt: "status".to_string(),
                model: "gpt-5".to_string(),
                prior_messages: Vec::new(),
                enable_tools: true,
            },
            &mut tools,
            &mut listener,
        )
        .await
        .expect("turn result");

        assert_eq!(tools.calls, vec!["session.status".to_string()]);
        assert_eq!(result.assistant.content, "status complete");
        assert!(listener.events.iter().any(|event| matches!(
            event,
            UiEvent::ToolExecutionFinished { tool_name, .. } if tool_name == "session.status"
        )));
    }
}
