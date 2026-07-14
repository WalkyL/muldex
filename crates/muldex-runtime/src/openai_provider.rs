use async_trait::async_trait;
use reqwest::Client;

use muldex_core::provider::InteractiveProvider;
use muldex_core::provider::ProviderAssistantTurn;
use muldex_core::provider::ProviderError;
use muldex_core::provider::ProviderEventSink;
use muldex_core::provider::ProviderMessageRole;
use muldex_core::provider::ProviderStreamEvent;
use muldex_core::provider::ProviderToolCall;
use muldex_core::provider::ProviderToolCallDelta;
use muldex_core::provider::ProviderTurnMessage;
use muldex_core::provider::ProviderTurnRequest;
use muldex_core::provider::ResolvedProviderConfig;

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleProvider {
    client: Client,
}

impl Default for OpenAiCompatibleProvider {
    fn default() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("reqwest client build"),
        }
    }
}

impl OpenAiCompatibleProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[derive(Debug, serde::Serialize)]
struct ChatCompletionsRequest<'a> {
    model: &'a str,
    messages: Vec<ChatCompletionsMessage<'a>>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ChatCompletionsTool<'a>>,
}

#[derive(Debug, serde::Serialize)]
struct ChatCompletionsMessage<'a> {
    role: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<ChatCompletionsToolCall<'a>>,
}

#[derive(Debug, serde::Serialize)]
struct ChatCompletionsToolCall<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    function: ChatCompletionsFunctionCall<'a>,
}

#[derive(Debug, serde::Serialize)]
struct ChatCompletionsFunctionCall<'a> {
    name: &'a str,
    arguments: &'a str,
}

#[derive(Debug, serde::Serialize)]
struct ChatCompletionsTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: ChatCompletionsToolFunction<'a>,
}

#[derive(Debug, serde::Serialize)]
struct ChatCompletionsToolFunction<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

#[derive(Debug, serde::Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, serde::Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct StreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<StreamToolCallDelta>>,
}

#[derive(Debug, serde::Deserialize)]
struct StreamToolCallDelta {
    index: usize,
    id: Option<String>,
    function: Option<StreamFunctionDelta>,
}

#[derive(Debug, serde::Deserialize)]
struct StreamFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Default)]
struct ToolCallAccumulator {
    id: String,
    name: String,
    arguments_json: String,
}

#[derive(serde::Deserialize)]
struct NonStreamChoice {
    message: NonStreamMessage,
}

#[derive(serde::Deserialize)]
struct NonStreamMessage {
    content: Option<String>,
}

#[derive(serde::Deserialize)]
struct NonStreamResponse {
    choices: Vec<NonStreamChoice>,
}

#[async_trait(?Send)]
impl InteractiveProvider for OpenAiCompatibleProvider {
    async fn run_turn(
        &self,
        provider: &ResolvedProviderConfig,
        request: ProviderTurnRequest,
        sink: &mut dyn ProviderEventSink,
    ) -> Result<ProviderAssistantTurn, ProviderError> {
        let payload = build_chat_completions_request(&request);
        let mut http_request = self
            .client
            .post(provider.chat_completions_url())
            .header("content-type", "application/json");

        if let Some(api_key) = provider.api_key.as_deref() {
            http_request = http_request.bearer_auth(api_key);
        }

        let response = http_request
            .json(&payload)
            .send()
            .await
            .map_err(|error| ProviderError::Http(error.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::Http(format!(
                "status {}",
                response.status()
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|error| ProviderError::Http(error.to_string()))?;

        let mut content = String::new();
        let mut tools: Vec<ToolCallAccumulator> = Vec::new();

        if request.stream {
            let mut buffer = String::new();
            buffer.push_str(&body);

            while let Some(split_index) = buffer.find("\n\n") {
                let frame = buffer[..split_index].to_string();
                buffer.drain(..split_index + 2);
                let trimmed = frame.trim();
                if trimmed.is_empty() {
                    continue;
                }
                for line in trimmed.lines() {
                    let line = line.trim();
                    if !line.starts_with("data:") {
                        continue;
                    }
                    let payload = line[5..].trim();
                    if payload == "[DONE]" {
                        sink.push(ProviderStreamEvent::MessageComplete);
                        continue;
                    }
                    let chunk: StreamChunk = serde_json::from_str(payload)
                        .map_err(|error| ProviderError::Protocol(error.to_string()))?;
                    apply_stream_chunk(chunk, sink, &mut content, &mut tools);
                }
            }
        } else {
            let result: NonStreamResponse = serde_json::from_str(&body)
                .map_err(|error| ProviderError::Protocol(error.to_string()))?;
            if let Some(choice) = result.choices.into_iter().next() {
                content = choice.message.content.unwrap_or_default();
                sink.push(ProviderStreamEvent::AssistantDelta(content.clone()));
                sink.push(ProviderStreamEvent::MessageComplete);
            }
        }

        let tool_calls = tools
            .into_iter()
            .map(|tool| ProviderToolCall {
                id: tool.id,
                name: tool.name,
                arguments_json: tool.arguments_json,
            })
            .collect::<Vec<_>>();

        Ok(ProviderAssistantTurn { content, tool_calls })
    }
}

fn build_chat_completions_request(request: &ProviderTurnRequest) -> ChatCompletionsRequest<'_> {
    ChatCompletionsRequest {
        model: &request.model,
        messages: request.messages.iter().map(map_message).collect(),
        stream: request.stream,
        tools: request.tools.iter().map(map_tool).collect(),
    }
}

fn map_message(message: &ProviderTurnMessage) -> ChatCompletionsMessage<'_> {
    ChatCompletionsMessage {
        role: match message.role {
            ProviderMessageRole::System => "system",
            ProviderMessageRole::User => "user",
            ProviderMessageRole::Assistant => "assistant",
            ProviderMessageRole::Tool => "tool",
        },
        content: message.content.as_deref(),
        tool_call_id: message.tool_call_id.as_deref(),
        name: message.name.as_deref(),
        tool_calls: message.tool_calls.iter().map(map_tool_call).collect(),
    }
}

fn map_tool_call(tool_call: &ProviderToolCall) -> ChatCompletionsToolCall<'_> {
    ChatCompletionsToolCall {
        id: &tool_call.id,
        kind: "function",
        function: ChatCompletionsFunctionCall {
            name: &tool_call.name,
            arguments: &tool_call.arguments_json,
        },
    }
}

fn map_tool(tool: &muldex_core::provider::ProviderToolSpec) -> ChatCompletionsTool<'_> {
    ChatCompletionsTool {
        kind: "function",
        function: ChatCompletionsToolFunction {
            name: &tool.name,
            description: &tool.description,
            parameters: &tool.input_schema,
        },
    }
}

fn apply_stream_chunk(
    chunk: StreamChunk,
    sink: &mut dyn ProviderEventSink,
    content: &mut String,
    tools: &mut Vec<ToolCallAccumulator>,
) {
    for choice in chunk.choices {
        if let Some(delta) = choice.delta.content {
            content.push_str(&delta);
            sink.push(ProviderStreamEvent::AssistantDelta(delta));
        }
        if let Some(tool_call_deltas) = choice.delta.tool_calls {
            for tool_delta in tool_call_deltas {
                while tools.len() <= tool_delta.index {
                    tools.push(ToolCallAccumulator::default());
                }
                let accumulator = &mut tools[tool_delta.index];
                let name_fragment = tool_delta.function.as_ref().and_then(|f| f.name.clone());
                let arguments_fragment = tool_delta
                    .function
                    .as_ref()
                    .and_then(|f| f.arguments.clone());
                if let Some(id) = tool_delta.id.clone() {
                    accumulator.id.push_str(&id);
                }
                if let Some(name) = name_fragment.clone() {
                    accumulator.name.push_str(&name);
                }
                if let Some(arguments) = arguments_fragment.clone() {
                    accumulator.arguments_json.push_str(&arguments);
                }
                sink.push(ProviderStreamEvent::ToolCallDelta(ProviderToolCallDelta {
                    index: tool_delta.index,
                    id_fragment: tool_delta.id,
                    name_fragment,
                    arguments_fragment,
                }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use muldex_core::provider::ProviderStreamEvent;
    use muldex_core::provider::ProviderToolSpec;

    #[derive(Default)]
    struct RecordingSink {
        events: Vec<ProviderStreamEvent>,
    }

    impl muldex_core::provider::ProviderEventSink for RecordingSink {
        fn push(&mut self, event: ProviderStreamEvent) {
            self.events.push(event);
        }
    }

    #[tokio::test]
    async fn request_payload_contains_model_messages_and_tools() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let address = listener.local_addr().expect("listener addr");

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let bytes = stream.read(&mut buffer).expect("read request");
                if bytes == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..bytes]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }

            let request_text = String::from_utf8_lossy(&request).to_string();
            let body = request_text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
            let response = concat!(
                "HTTP/1.1 200 OK\r\n",
                "content-type: text/event-stream\r\n",
                "transfer-encoding: chunked\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).expect("write headers");
            let payload = "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\n\
data: [DONE]\n\n";
            let chunk = format!("{:X}\r\n{}\r\n0\r\n\r\n", payload.len(), payload);
            stream.write_all(chunk.as_bytes()).expect("write body");
            stream.flush().expect("flush response");
            body
        });

        let provider = OpenAiCompatibleProvider::default();
        let resolved = ResolvedProviderConfig {
            name: "llm-router".to_string(),
            kind: "openai-compatible".to_string(),
            base_url: format!("http://{address}/v1"),
            api_key: Some("secret".to_string()),
            default_model: Some("gpt-5".to_string()),
        };
        let request = ProviderTurnRequest {
            model: "gpt-5".to_string(),
            messages: vec![ProviderTurnMessage {
                role: ProviderMessageRole::User,
                content: Some("hello".to_string()),
                tool_call_id: None,
                name: None,
                tool_calls: Vec::new(),
            }],
            stream: true,
            tools: vec![ProviderToolSpec {
                name: "session.status".to_string(),
                description: "Show session status".to_string(),
                input_schema: serde_json::json!({"type":"object","properties":{}}),
            }],
        };
        let mut sink = RecordingSink::default();

        let turn = provider
            .run_turn(&resolved, request, &mut sink)
            .await
            .expect("run turn");

        let body = handle.join().expect("join server thread");
        assert!(body.contains("\"model\":\"gpt-5\""));
        assert!(body.contains("\"messages\""));
        assert!(body.contains("\"tools\""));
        assert_eq!(turn.content, "hello");
    }

    #[tokio::test]
    async fn streaming_deltas_are_parsed_into_sink_events() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let address = listener.local_addr().expect("listener addr");

        let _handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut buffer = [0u8; 4096];
            loop {
                let bytes = stream.read(&mut buffer).expect("read request");
                if bytes == 0 {
                    break;
                }
                if String::from_utf8_lossy(&buffer[..bytes]).contains("\r\n\r\n") {
                    break;
                }
            }
            let response = concat!(
                "HTTP/1.1 200 OK\r\n",
                "content-type: text/event-stream\r\n",
                "transfer-encoding: chunked\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).expect("write headers");
            let payload = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"lo\",\"tool_calls\":[{\"index\":0,\"id\":\"call_\",\"function\":{\"name\":\"session.\",\"arguments\":\"{\"}}]}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"1\",\"function\":{\"name\":\"status\",\"arguments\":\"}\"}}]}}]}\n\n",
                "data: [DONE]\n\n"
            );
            let chunk = format!("{:X}\r\n{}\r\n0\r\n\r\n", payload.len(), payload);
            stream.write_all(chunk.as_bytes()).expect("write body");
            stream.flush().expect("flush body");
        });

        let provider = OpenAiCompatibleProvider::default();
        let resolved = ResolvedProviderConfig {
            name: "llm-router".to_string(),
            kind: "openai-compatible".to_string(),
            base_url: format!("http://{address}/v1"),
            api_key: None,
            default_model: None,
        };
        let request = ProviderTurnRequest {
            model: "gpt-5".to_string(),
            messages: vec![ProviderTurnMessage {
                role: ProviderMessageRole::User,
                content: Some("hello".to_string()),
                tool_call_id: None,
                name: None,
                tool_calls: Vec::new(),
            }],
            stream: true,
            tools: Vec::new(),
        };
        let mut sink = RecordingSink::default();

        let turn = provider
            .run_turn(&resolved, request, &mut sink)
            .await
            .expect("run turn");

        assert_eq!(turn.content, "hello");
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].id, "call_1");
        assert_eq!(turn.tool_calls[0].name, "session.status");
        assert_eq!(turn.tool_calls[0].arguments_json, "{}");
        assert!(sink.events.iter().any(|event| matches!(
            event,
            ProviderStreamEvent::AssistantDelta(delta) if delta == "hel"
        )));
        assert!(sink.events.iter().any(|event| matches!(
            event,
            ProviderStreamEvent::ToolCallDelta(delta) if delta.index == 0
        )));
    }
}
