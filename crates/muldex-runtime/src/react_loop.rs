use serde::Deserialize;
use serde::Serialize;

use muldex_core::provider::ProviderAssistantTurn;
use muldex_core::provider::ProviderToolCall;
use muldex_core::provider::ProviderTurnMessage;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolExecutionRecord {
    pub call: ProviderToolCall,
    pub result: String,
}

pub fn append_tool_result_message(
    messages: &mut Vec<ProviderTurnMessage>,
    tool_name: &str,
    tool_call_id: &str,
    result: String,
) {
    messages.push(ProviderTurnMessage {
        role: muldex_core::provider::ProviderMessageRole::Tool,
        content: Some(result),
        tool_call_id: Some(tool_call_id.to_string()),
        name: Some(tool_name.to_string()),
        tool_calls: Vec::new(),
    });
}

pub fn assistant_turn_requires_tool_loop(turn: &ProviderAssistantTurn) -> bool {
    !turn.tool_calls.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use muldex_core::provider::ProviderMessageRole;

    #[test]
    fn tool_result_message_is_added_as_tool_role_message() {
        let mut messages = vec![];
        append_tool_result_message(
            &mut messages,
            "session.status",
            "call_1",
            "{\"ok\":true}".to_string(),
        );

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, ProviderMessageRole::Tool);
        assert_eq!(messages[0].name.as_deref(), Some("session.status"));
        assert_eq!(messages[0].tool_call_id.as_deref(), Some("call_1"));
    }

    #[test]
    fn assistant_turn_requires_tool_loop_when_tool_calls_exist() {
        let turn = ProviderAssistantTurn {
            content: String::new(),
            tool_calls: vec![ProviderToolCall {
                id: "call_1".to_string(),
                name: "session.status".to_string(),
                arguments_json: "{}".to_string(),
            }],
        };

        assert!(assistant_turn_requires_tool_loop(&turn));
    }
}
