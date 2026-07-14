use async_trait::async_trait;
use reqwest::Client;


use muldex_core::provider::InteractiveProvider;
use muldex_core::provider::ProviderAssistantTurn;
use muldex_core::provider::ProviderError;
use muldex_core::provider::ProviderEventSink;
use muldex_core::provider::ProviderMessageRole;
use muldex_core::provider::ProviderRateLimit;
use muldex_core::provider::ProviderUsage;
use muldex_core::provider::ProviderStreamEvent;
use muldex_core::provider::ProviderToolCall;
use muldex_core::provider::ProviderToolCallDelta;
use muldex_core::provider::ProviderTurnMessage;
use muldex_core::provider::ProviderTurnRequest;
use muldex_core::provider::ResolvedProviderConfig;

#[derive(Debug, Clone)]
pub struct ResponsesProvider {
    client: Client,
}

impl Default for ResponsesProvider {
    fn default() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(180))
                .no_proxy()
                .build()
                .expect("reqwest client build"),
        }
    }
}

impl ResponsesProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[derive(Debug, serde::Serialize)]
struct ResponsesRequest<'a> {
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<&'a str>,
    input: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ResponsesTool<'a>>,
    tool_choice: &'a str,
    stream: bool,
    include: Vec<&'a str>,
}

#[derive(Debug, serde::Serialize)]
struct ResponsesTool<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    name: &'a str,
    description: &'a str,
    input_schema: &'a serde_json::Value,
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type")]
enum ResponsesStreamEvent {
    #[serde(rename = "response.created")]
    ResponseCreated,
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded {
        item: ResponsesOutputItem,
    },
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded,
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        delta: String,
        #[allow(dead_code)]
        item_id: String,
        #[allow(dead_code)]
        output_index: usize,
        #[allow(dead_code)]
        content_index: usize,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        #[allow(dead_code)]
        item_id: String,
    },
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta {
        delta: String,
        #[allow(dead_code)]
        item_id: String,
        #[allow(dead_code)]
        #[serde(default)]
        output_index: usize,
    },
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone {
        #[allow(dead_code)]
        item_id: String,
        #[allow(dead_code)]
        #[serde(default)]
        output_index: usize,
    },
    #[serde(rename = "response.completed")]
    ResponseCompleted,
    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        message: Option<String>,
    },
}

#[derive(Debug, serde::Deserialize)]
struct ResponsesOutputItem {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    call_id: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    status: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ResponsesNonStreamingResponse {
    #[allow(dead_code)]
    id: String,
    output: Vec<ResponsesOutputItem>,
}

#[derive(Debug, serde::Deserialize)]
struct ResponsesCompleted {
    response: ResponsesUsageContainer,
}

#[derive(Debug, serde::Deserialize)]
struct ResponsesUsageContainer {
    #[serde(default)]
    usage: Option<ResponsesUsage>,
}

#[derive(Debug, serde::Deserialize)]
struct ResponsesUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cached_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

fn parse_rate_limit(headers: &reqwest::header::HeaderMap) -> ProviderRateLimit {
    fn atoi(header: &reqwest::header::HeaderMap, name: &str) -> Option<u64> {
        header
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
    }
    ProviderRateLimit {
        limit_requests: atoi(headers, "x-ratelimit-limit-requests"),
        remaining_requests: atoi(headers, "x-ratelimit-remaining-requests"),
        limit_tokens: atoi(headers, "x-ratelimit-limit-tokens"),
        remaining_tokens: atoi(headers, "x-ratelimit-remaining-tokens"),
        reset_after_seconds: atoi(headers, "x-ratelimit-reset-requests")
            .or_else(|| atoi(headers, "retry-after")),
    }
}

struct ToolCallAccumulator {
    item_id: String,
    call_id: String,
    name: String,
    arguments_json: String,
}

#[async_trait(?Send)]
impl InteractiveProvider for ResponsesProvider {
    async fn run_turn(
        &self,
        provider: &ResolvedProviderConfig,
        request: ProviderTurnRequest,
        sink: &mut dyn ProviderEventSink,
    ) -> Result<ProviderAssistantTurn, ProviderError> {
        let payload = build_responses_request(&request);
        let url = responses_url(&provider.base_url);

        let mut http_request = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream");

        if let Some(api_key) = provider.api_key.as_deref() {
            http_request = http_request.bearer_auth(api_key);
        }

        let response = http_request
            .json(&payload)
            .send()
            .await
            .map_err(|error| {
                let detail = if error.is_timeout() {
                    format!("timeout: {error}")
                } else if error.is_connect() {
                    format!("connect: {error}")
                } else if error.is_status() {
                    format!("status: {error}")
                } else {
                    format!("other: {error}")
                };
                ProviderError::Http(detail)
            })?;

        if !response.status().is_success() {
            return Err(ProviderError::Http(format!(
                "status {}",
                response.status()
            )));
        }

        sink.push(ProviderStreamEvent::RateLimit(parse_rate_limit(
            response.headers(),
        )));

        let body = response
            .text()
            .await
            .map_err(|error| ProviderError::Http(error.to_string()))?;

        let mut content = String::new();
        let mut tools: Vec<ToolCallAccumulator> = Vec::new();

        if request.stream {
            parse_sse_stream(&body, sink, &mut content, &mut tools)?;
        } else {
            let result: ResponsesNonStreamingResponse = serde_json::from_str(&body)
                .map_err(|error| ProviderError::Protocol(error.to_string()))?;
            for item in result.output {
                if item.kind == "function_call" {
                    let call_id = item.call_id.unwrap_or_default();
                    let name = item.name.unwrap_or_default();
                    tools.push(ToolCallAccumulator {
                        item_id: item.id,
                        call_id,
                        name,
                        arguments_json: String::new(),
                    });
                }
            }
            if !content.is_empty() {
                sink.push(ProviderStreamEvent::AssistantDelta(content.clone()));
            }
            sink.push(ProviderStreamEvent::MessageComplete);
        }

        let tool_calls = tools
            .into_iter()
            .map(|tool| ProviderToolCall {
                id: tool.call_id,
                name: tool.name,
                arguments_json: tool.arguments_json,
            })
            .collect::<Vec<_>>();

        Ok(ProviderAssistantTurn { content, tool_calls })
    }
}

fn responses_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/responses") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/responses")
    }
}

fn build_responses_request(request: &ProviderTurnRequest) -> ResponsesRequest<'_> {
    ResponsesRequest {
        model: &request.model,
        instructions: None,
        input: map_messages_to_input(&request.messages),
        tools: request.tools.iter().map(map_tool).collect(),
        tool_choice: "auto",
        stream: request.stream,
        include: vec!["reasoning.encrypted_content"],
    }
}

fn map_messages_to_input(messages: &[ProviderTurnMessage]) -> Vec<serde_json::Value> {
    let mut items = Vec::new();
    for msg in messages {
        match msg.role {
            ProviderMessageRole::System
            | ProviderMessageRole::User
            | ProviderMessageRole::Assistant => {
                let role_str = match msg.role {
                    ProviderMessageRole::System => "system",
                    ProviderMessageRole::User => "user",
                    ProviderMessageRole::Assistant => "assistant",
                    _ => unreachable!(),
                };
                items.push(serde_json::json!({
                    "type": "message",
                    "role": role_str,
                    "content": [{
                        "type": "input_text",
                        "text": msg.content.as_deref().unwrap_or("")
                    }]
                }));
                if matches!(msg.role, ProviderMessageRole::Assistant) {
                    for tc in &msg.tool_calls {
                        items.push(serde_json::json!({
                            "type": "function_call",
                            "call_id": tc.id,
                            "name": tc.name,
                            "arguments": tc.arguments_json,
                        }));
                    }
                }
            }
            ProviderMessageRole::Tool => {
                items.push(serde_json::json!({
                    "type": "function_call_output",
                    "call_id": msg.tool_call_id.as_deref().unwrap_or(""),
                    "output": msg.content.as_deref().unwrap_or(""),
                }));
            }
        }
    }
    items
}

fn map_tool(tool: &muldex_core::provider::ProviderToolSpec) -> ResponsesTool<'_> {
    ResponsesTool {
        kind: "function",
        name: &tool.name,
        description: &tool.description,
        input_schema: &tool.input_schema,
    }
}

fn parse_sse_stream(
    body: &str,
    sink: &mut dyn ProviderEventSink,
    content: &mut String,
    tools: &mut Vec<ToolCallAccumulator>,
) -> Result<(), ProviderError> {
    let mut buffer = String::new();
    buffer.push_str(body);

    while let Some(split_index) = buffer.find("\n\n") {
        let frame = buffer[..split_index].to_string();
        buffer.drain(..split_index + 2);
        let trimmed = frame.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut event_name: Option<String> = None;
        let mut data_json: Option<String> = None;

        for line in trimmed.lines() {
            let line = line.trim();
            if let Some(name) = line.strip_prefix("event:") {
                event_name = Some(name.trim().to_string());
            } else if let Some(json) = line.strip_prefix("data:") {
                data_json = Some(json.trim().to_string());
            }
        }

        let Some(json) = data_json else {
            continue;
        };

        let Some(event) = event_name else {
            continue;
        };

        match event.as_str() {
            "response.output_text.delta" => {
                if let Ok(delta_event) = serde_json::from_str::<ResponsesStreamEvent>(&json) {
                    if let ResponsesStreamEvent::OutputTextDelta { delta, .. } = delta_event {
                        content.push_str(&delta);
                        sink.push(ProviderStreamEvent::AssistantDelta(delta));
                    }
                }
            }
            "response.output_item.added" => {
                if let Ok(item_event) = serde_json::from_str::<ResponsesStreamEvent>(&json) {
                    if let ResponsesStreamEvent::OutputItemAdded { item } = item_event {
                        if item.kind == "function_call" {
                            let call_id = item.call_id.unwrap_or_default();
                            let name = item.name.unwrap_or_default();
                            if !tools.iter().any(|t| t.item_id == item.id) {
                                tools.push(ToolCallAccumulator {
                                    item_id: item.id,
                                    call_id,
                                    name,
                                    arguments_json: String::new(),
                                });
                            }
                        }
                    }
                }
            }
            "response.function_call_arguments.delta" => {
                if let Ok(arg_event) = serde_json::from_str::<ResponsesStreamEvent>(&json) {
                    if let ResponsesStreamEvent::FunctionCallArgumentsDelta {
                        delta,
                        item_id,
                        ..
                    } = arg_event
                    {
                        let index =
                            if let Some(pos) = tools.iter().position(|t| t.item_id == item_id) {
                                pos
                            } else {
                                tools.push(ToolCallAccumulator {
                                    item_id,
                                    call_id: String::new(),
                                    name: String::new(),
                                    arguments_json: String::new(),
                                });
                                tools.len() - 1
                            };
                        tools[index].arguments_json.push_str(&delta);
                        sink.push(ProviderStreamEvent::ToolCallDelta(ProviderToolCallDelta {
                            index,
                            id_fragment: None,
                            name_fragment: None,
                            arguments_fragment: Some(delta),
                        }));
                    }
                }
            }
            "response.completed" => {
                if let Ok(completed) = serde_json::from_str::<ResponsesCompleted>(&json) {
                    if let Some(usage) = completed.response.usage {
                        sink.push(ProviderStreamEvent::Usage(ProviderUsage {
                            input_tokens: usage.input_tokens,
                            cached_input_tokens: usage.cached_tokens,
                            output_tokens: usage.output_tokens,
                            total_tokens: usage.total_tokens,
                        }));
                    }
                }
                sink.push(ProviderStreamEvent::MessageComplete);
            }
            "error" => {
                let msg =
                    if let Ok(err_event) = serde_json::from_str::<ResponsesStreamEvent>(&json) {
                        if let ResponsesStreamEvent::Error { message } = err_event {
                            message.unwrap_or_else(|| "unknown error".to_string())
                        } else {
                            "unknown error".to_string()
                        }
                    } else {
                        "unknown error".to_string()
                    };
                return Err(ProviderError::Protocol(msg));
            }
            _ => {}
        }
    }

    Ok(())
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
    async fn request_payload_contains_model_input_and_tools() {
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
            let body = request_text
                .split("\r\n\r\n")
                .nth(1)
                .unwrap_or("")
                .to_string();
            let response = concat!(
                "HTTP/1.1 200 OK\r\n",
                "content-type: text/event-stream\r\n",
                "transfer-encoding: chunked\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).expect("write headers");
            let payload = "event: response.created\ndata: {\"type\":\"response.created\"}\n\n\
                 event: response.completed\ndata: {\"type\":\"response.completed\"}\n\n";
            let chunk = format!("{:X}\r\n{}\r\n0\r\n\r\n", payload.len(), payload);
            stream.write_all(chunk.as_bytes()).expect("write body");
            stream.flush().expect("flush response");
            body
        });

        let provider = ResponsesProvider::default();
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

        let _turn = provider
            .run_turn(&resolved, request, &mut sink)
            .await
            .expect("run turn");

        let body = handle.join().expect("join server thread");
        assert!(body.contains("\"model\":\"gpt-5\""));
        assert!(body.contains("\"input\""));
        assert!(body.contains("\"tools\""));
        assert!(body.contains("\"tool_choice\":\"auto\""));
        assert!(body.contains("\"stream\":true"));
        assert!(body.contains("\"include\""));
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
                "event: response.created\ndata: {\"type\":\"response.created\"}\n\n",
                "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"item\":{\"id\":\"msg_1\",\"type\":\"message\",\"status\":\"in_progress\"}}\n\n",
                "event: response.content_part.added\ndata: {\"type\":\"response.content_part.added\"}\n\n",
                "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"Hel\",\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0}\n\n",
                "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"lo\",\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0}\n\n",
                "event: response.output_text.done\ndata: {\"type\":\"response.output_text.done\",\"item_id\":\"msg_1\"}\n\n",
                "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"item\":{\"id\":\"fc_1\",\"type\":\"function_call\",\"name\":\"session.status\",\"call_id\":\"call_abc\",\"status\":\"completed\"}}\n\n",
                "event: response.function_call_arguments.delta\ndata: {\"type\":\"response.function_call_arguments.delta\",\"delta\":\"{}\",\"item_id\":\"fc_1\",\"output_index\":0,\"content_index\":0}\n\n",
                "event: response.function_call_arguments.done\ndata: {\"type\":\"response.function_call_arguments.done\",\"item_id\":\"fc_1\",\"output_index\":0}\n\n",
                "event: response.completed\ndata: {\"type\":\"response.completed\"}\n\n"
            );
            let chunk = format!("{:X}\r\n{}\r\n0\r\n\r\n", payload.len(), payload);
            stream.write_all(chunk.as_bytes()).expect("write body");
            stream.flush().expect("flush body");
        });

        let provider = ResponsesProvider::default();
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

        assert_eq!(turn.content, "Hello");
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].id, "call_abc");
        assert_eq!(turn.tool_calls[0].name, "session.status");
        assert_eq!(turn.tool_calls[0].arguments_json, "{}");
        assert!(sink.events.iter().any(|event| matches!(
            event,
            ProviderStreamEvent::AssistantDelta(delta) if delta == "Hel"
        )));
        assert!(sink.events.iter().any(|event| matches!(
            event,
            ProviderStreamEvent::ToolCallDelta(delta) if delta.index == 0
        )));
    }

    #[tokio::test]
    async fn error_event_returns_protocol_error() {
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
            let payload =
                "event: error\ndata: {\"type\":\"error\",\"message\":\"rate limit exceeded\"}\n\n";
            let chunk = format!("{:X}\r\n{}\r\n0\r\n\r\n", payload.len(), payload);
            stream.write_all(chunk.as_bytes()).expect("write body");
            stream.flush().expect("flush body");
        });

        let provider = ResponsesProvider::default();
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

        let result = provider.run_turn(&resolved, request, &mut sink).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("rate limit exceeded"));
    }

    #[tokio::test]
    async fn completed_event_with_usage_emits_usage_event() {
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
                "x-ratelimit-remaining-requests: 42\r\n",
                "x-ratelimit-limit-requests: 100\r\n",
                "transfer-encoding: chunked\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).expect("write headers");
            let payload = concat!(
                "event: response.created\ndata: {\"type\":\"response.created\"}\n\n",
                "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":10,\"cached_tokens\":2,\"output_tokens\":3,\"total_tokens\":13}}}\n\n"
            );
            let chunk = format!("{:X}\r\n{}\r\n0\r\n\r\n", payload.len(), payload);
            stream.write_all(chunk.as_bytes()).expect("write body");
            stream.flush().expect("flush body");
        });

        let provider = ResponsesProvider::default();
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

        let _ = provider.run_turn(&resolved, request, &mut sink).await;

        assert!(sink
            .events
            .iter()
            .any(|event| matches!(event, ProviderStreamEvent::Usage(usage) if usage
                .total_tokens
                == 13)));
        assert!(sink.events.iter().any(|event| matches!(
            event,
            ProviderStreamEvent::RateLimit(rl) if rl.remaining_requests == Some(42)
                && rl.limit_requests == Some(100)
        )));
    }
}