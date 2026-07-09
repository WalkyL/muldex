use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClientAccessMode {
    ReadOnly,
    Full,
}

pub fn client_command_allowed(access_mode: &ClientAccessMode, command_kind: &str) -> bool {
    match access_mode {
        ClientAccessMode::Full => true,
        ClientAccessMode::ReadOnly => matches!(command_kind, "status"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_client_allows_status_only() {
        assert!(client_command_allowed(&ClientAccessMode::ReadOnly, "status"));
        assert!(!client_command_allowed(
            &ClientAccessMode::ReadOnly,
            "advance-sample"
        ));
    }

    #[test]
    fn full_client_allows_all_current_commands() {
        assert!(client_command_allowed(&ClientAccessMode::Full, "status"));
        assert!(client_command_allowed(&ClientAccessMode::Full, "advance-sample"));
    }
}
