use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde::Deserialize;
use serde::Serialize;

use crate::host::RuntimeHost;
use crate::runtime::RuntimeCommand;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonCommandEnvelope {
    pub schema_version: String,
    pub command_id: String,
    pub session_id: Option<String>,
    pub command_name: String,
    pub payload_kind: String,
    pub payload_json: String,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonResponseEnvelope {
    pub schema_version: String,
    pub command_id: String,
    pub ok: bool,
    pub payload_kind: String,
    pub payload_json: String,
    pub error: Option<String>,
    pub created_at_ms: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DaemonTransportError {
    #[error("daemon transport IO failed: {0}")]
    Io(String),
    #[error("daemon transport serialization failed: {0}")]
    Serialization(String),
    #[error("daemon transport item not found: {0}")]
    NotFound(String),
    #[error("daemon transport host error: {0}")]
    Host(String),
}

#[derive(Debug, Clone)]
pub struct FileCommandTransport {
    root: PathBuf,
    commands_dir: PathBuf,
    archive_dir: PathBuf,
    responses_dir: PathBuf,
}

impl FileCommandTransport {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let commands_dir = root.join("commands");
        let archive_dir = root.join("commands-archive");
        let responses_dir = root.join("responses");
        Self {
            root,
            commands_dir,
            archive_dir,
            responses_dir,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn commands_dir(&self) -> &Path {
        &self.commands_dir
    }

    pub fn responses_dir(&self) -> &Path {
        &self.responses_dir
    }

    pub fn archive_dir(&self) -> &Path {
        &self.archive_dir
    }

    pub fn ensure_dirs(&self) -> Result<(), DaemonTransportError> {
        fs::create_dir_all(&self.commands_dir)
            .map_err(|error| DaemonTransportError::Io(error.to_string()))?;
        fs::create_dir_all(&self.archive_dir)
            .map_err(|error| DaemonTransportError::Io(error.to_string()))?;
        fs::create_dir_all(&self.responses_dir)
            .map_err(|error| DaemonTransportError::Io(error.to_string()))?;
        Ok(())
    }

    pub fn write_command(
        &self,
        envelope: &DaemonCommandEnvelope,
    ) -> Result<PathBuf, DaemonTransportError> {
        self.ensure_dirs()?;
        let path = self
            .commands_dir
            .join(format!("{}.json", envelope.command_id));
        let json = serde_json::to_string_pretty(envelope)
            .map_err(|error| DaemonTransportError::Serialization(error.to_string()))?;
        fs::write(&path, json).map_err(|error| DaemonTransportError::Io(error.to_string()))?;
        Ok(path)
    }

    pub fn write_response(
        &self,
        envelope: &DaemonResponseEnvelope,
    ) -> Result<PathBuf, DaemonTransportError> {
        self.ensure_dirs()?;
        let path = self
            .responses_dir
            .join(format!("{}.json", envelope.command_id));
        let json = serde_json::to_string_pretty(envelope)
            .map_err(|error| DaemonTransportError::Serialization(error.to_string()))?;
        fs::write(&path, json).map_err(|error| DaemonTransportError::Io(error.to_string()))?;
        Ok(path)
    }

    pub fn list_commands(&self) -> Result<Vec<DaemonCommandEnvelope>, DaemonTransportError> {
        self.ensure_dirs()?;
        let mut items = Vec::new();
        for entry in fs::read_dir(&self.commands_dir)
            .map_err(|error| DaemonTransportError::Io(error.to_string()))?
        {
            let entry = entry.map_err(|error| DaemonTransportError::Io(error.to_string()))?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let json = fs::read_to_string(&path)
                .map_err(|error| DaemonTransportError::Io(error.to_string()))?;
            let envelope: DaemonCommandEnvelope = serde_json::from_str(&json)
                .map_err(|error| DaemonTransportError::Serialization(error.to_string()))?;
            items.push(envelope);
        }
        items.sort_by(|left, right| left.command_id.cmp(&right.command_id));
        Ok(items)
    }

    pub fn read_response(
        &self,
        command_id: &str,
    ) -> Result<DaemonResponseEnvelope, DaemonTransportError> {
        self.ensure_dirs()?;
        let path = self.responses_dir.join(format!("{}.json", command_id));
        if !path.exists() {
            return Err(DaemonTransportError::NotFound(command_id.to_string()));
        }
        let json = fs::read_to_string(&path)
            .map_err(|error| DaemonTransportError::Io(error.to_string()))?;
        serde_json::from_str(&json)
            .map_err(|error| DaemonTransportError::Serialization(error.to_string()))
    }

    pub fn archive_command(&self, command_id: &str) -> Result<PathBuf, DaemonTransportError> {
        self.ensure_dirs()?;
        let source = self.commands_dir.join(format!("{}.json", command_id));
        if !source.exists() {
            return Err(DaemonTransportError::NotFound(command_id.to_string()));
        }
        let target = self.archive_dir.join(format!("{}.json", command_id));
        fs::rename(&source, &target).map_err(|error| DaemonTransportError::Io(error.to_string()))?;
        Ok(target)
    }

    pub fn process_commands(
        &self,
        host: &mut RuntimeHost,
    ) -> Result<Vec<DaemonResponseEnvelope>, DaemonTransportError> {
        let commands = self.list_commands()?;
        let mut responses = Vec::new();

        for command in commands {
            let result = if command.schema_version != "daemon-envelope-v1" {
                Err(DaemonTransportError::Serialization(format!(
                    "unsupported daemon envelope schema_version: {}",
                    command.schema_version
                )))
            } else if command.payload_kind != "RuntimeCommand" {
                Err(DaemonTransportError::Serialization(format!(
                    "unsupported daemon command payload_kind: {}",
                    command.payload_kind
                )))
            } else {
                let runtime_command: Result<RuntimeCommand, DaemonTransportError> =
                    serde_json::from_str(&command.payload_json)
                        .map_err(|error| DaemonTransportError::Serialization(error.to_string()));

                match (command.session_id.as_deref(), runtime_command) {
                    (Some(session_id), Ok(runtime_command)) => host
                        .apply_command(session_id, runtime_command)
                        .map_err(|error| DaemonTransportError::Host(error.to_string())),
                    (None, _) => Err(DaemonTransportError::Host(
                        "command missing session_id".to_string(),
                    )),
                    (_, Err(error)) => Err(error),
                }
            };

            let response = match result {
                Ok(runtime_result) => DaemonResponseEnvelope {
                    schema_version: "daemon-envelope-v1".to_string(),
                    command_id: command.command_id.clone(),
                    ok: true,
                    payload_kind: "RuntimeCommandResult".to_string(),
                    payload_json: serde_json::to_string_pretty(&runtime_result)
                        .map_err(|error| DaemonTransportError::Serialization(error.to_string()))?,
                    error: None,
                    created_at_ms: now_ms(),
                },
                Err(error) => DaemonResponseEnvelope {
                    schema_version: "daemon-envelope-v1".to_string(),
                    command_id: command.command_id.clone(),
                    ok: false,
                    payload_kind: "Error".to_string(),
                    payload_json: String::new(),
                    error: Some(error.to_string()),
                    created_at_ms: now_ms(),
                },
            };

            self.write_response(&response)?;
            self.archive_command(&command.command_id)?;
            responses.push(response);
        }

        Ok(responses)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::RuntimeHost;
    use crate::runtime::RuntimeCommand;
    use crate::runtime::RuntimeCommandResult;
    use muldex_core::protocol::ApprovalPolicyDescriptor;
    use muldex_core::protocol::ContextPressure;
    use muldex_core::protocol::ContinueDecision;
    use muldex_core::protocol::ContinueMode;
    use muldex_core::protocol::ContinueReason;
    use muldex_core::protocol::ContinueRequest;
    use muldex_core::protocol::ExecutionMode;
    use muldex_core::protocol::InterruptQueueState;
    use muldex_core::protocol::PendingApprovalState;
    use muldex_core::protocol::PermissionContextSnapshot;
    use muldex_core::protocol::PostCompactionState;
    use muldex_core::protocol::ProgressSnapshot;
    use muldex_core::protocol::RecoverySnapshot;
    use muldex_core::protocol::RuntimeModeState;
    use muldex_core::protocol::SandboxModeDescriptor;
    use muldex_core::protocol::SelfCorrectionState;
    use muldex_core::protocol::StateChangeKind;

    fn sample_host() -> RuntimeHost {
        let mut host = RuntimeHost::new();
        host.create_session(
            "session-1",
            crate::runtime::RuntimeState {
                request: ContinueRequest {
                    thread_id: "thread-1".to_string(),
                    turn_id: "turn-1".to_string(),
                    objective: "continue task".to_string(),
                    constraints: vec!["do not spin".to_string()],
                    continue_reason: ContinueReason::PendingInput,
                    recent_state_changes: vec![StateChangeKind::NewConfirmedFinding],
                    working_hypothesis: None,
                    last_agent_message: None,
                    pending_input_count: 0,
                    trigger_turn_pending: false,
                    tool_call_count_this_turn: 0,
                    context_pressure: ContextPressure::default(),
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
                    pending_approval: PendingApprovalState::default(),
                    interrupts: InterruptQueueState::default(),
                    last_run_report: None,
                    safety: PermissionContextSnapshot {
                        sandbox_mode: SandboxModeDescriptor::WorkspaceWrite,
                        approval_policy: ApprovalPolicyDescriptor::OnRequest,
                        permission_profile_summary: "managed".to_string(),
                        network_access_enabled: false,
                        requires_explicit_approval_for_next_step: false,
                    },
                    codex_continuation: None,
                },
                cycle_index: 0,
                phase: crate::runtime::RuntimePhase::Ready,
                latest_report: None,
            },
        )
        .expect("create session");
        host
    }

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn transport_can_round_trip_command_and_response() {
        let root = root("muldex-daemon-transport-roundtrip");
        let transport = FileCommandTransport::new(&root);

        let command = DaemonCommandEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-1".to_string(),
            session_id: Some("session-1".to_string()),
            command_name: "apply_command".to_string(),
            payload_kind: "RuntimeCommand".to_string(),
            payload_json: "{\"kind\":\"status\"}".to_string(),
            created_at_ms: 1,
        };
        let response = DaemonResponseEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-1".to_string(),
            ok: true,
            payload_kind: "RuntimeCommandResult".to_string(),
            payload_json: "{\"status\":\"ok\"}".to_string(),
            error: None,
            created_at_ms: 2,
        };

        transport.write_command(&command).expect("write command");
        transport.write_response(&response).expect("write response");

        let commands = transport.list_commands().expect("list commands");
        let loaded_response = transport.read_response("cmd-1").expect("read response");

        assert_eq!(commands, vec![command]);
        assert_eq!(loaded_response, response);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn transport_reports_missing_response() {
        let root = root("muldex-daemon-transport-missing");
        let transport = FileCommandTransport::new(&root);
        let error = transport.read_response("missing").expect_err("missing response");

        assert_eq!(error, DaemonTransportError::NotFound("missing".to_string()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn transport_can_process_command_against_host() {
        let root = root("muldex-daemon-transport-process");
        let transport = FileCommandTransport::new(&root);
        let mut host = sample_host();

        let runtime_command = RuntimeCommand::Decision(ContinueDecision {
            allow_continue: true,
            mode: ContinueMode::NextTurn,
            rationale: "advance through transport".to_string(),
            next_action: None,
            pause_for_approval: false,
            consume_interrupts_now: false,
            may_continue_other_work: true,
            suppress_duplicate_injection: false,
            downgrade_trigger_turn: false,
            request_compaction: false,
            request_handoff_summary: false,
            request_checkpoint: false,
            enter_self_correction: false,
        });
        let payload_json = serde_json::to_string_pretty(&runtime_command).expect("serialize command");
        let envelope = DaemonCommandEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-transport-1".to_string(),
            session_id: Some("session-1".to_string()),
            command_name: "apply_command".to_string(),
            payload_kind: "RuntimeCommand".to_string(),
            payload_json,
            created_at_ms: 1,
        };

        transport.write_command(&envelope).expect("write command");
        let responses = transport.process_commands(&mut host).expect("process command");

        assert_eq!(responses.len(), 1);
        assert!(responses[0].ok);

        let decoded: RuntimeCommandResult =
            serde_json::from_str(&responses[0].payload_json).expect("decode runtime result");
        match decoded {
            RuntimeCommandResult::Step(step) => {
                assert_eq!(step.updated_state.cycle_index, 1);
            }
            _ => panic!("expected step result"),
        }

        assert!(!transport.commands_dir().join("cmd-transport-1.json").exists());
        assert!(transport.archive_dir().join("cmd-transport-1.json").exists());
        assert!(transport.responses_dir().join("cmd-transport-1.json").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn transport_rejects_unsupported_command_schema_with_error_response() {
        let root = root("muldex-daemon-transport-bad-schema");
        let transport = FileCommandTransport::new(&root);
        let mut host = sample_host();

        let envelope = DaemonCommandEnvelope {
            schema_version: "daemon-envelope-v0".to_string(),
            command_id: "cmd-bad-schema".to_string(),
            session_id: Some("session-1".to_string()),
            command_name: "apply_command".to_string(),
            payload_kind: "RuntimeCommand".to_string(),
            payload_json: "{}".to_string(),
            created_at_ms: 1,
        };

        transport.write_command(&envelope).expect("write command");
        let responses = transport.process_commands(&mut host).expect("process command");

        assert_eq!(responses.len(), 1);
        assert!(!responses[0].ok);
        assert_eq!(responses[0].payload_kind, "Error");
        assert!(responses[0]
            .error
            .as_deref()
            .expect("error")
            .contains("unsupported daemon envelope schema_version"));
        assert!(!transport.commands_dir().join("cmd-bad-schema.json").exists());
        assert!(transport.archive_dir().join("cmd-bad-schema.json").exists());
        assert!(transport.responses_dir().join("cmd-bad-schema.json").exists());
        assert_eq!(host.get_state("session-1").expect("state").cycle_index, 0);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn transport_rejects_unsupported_command_payload_kind_with_error_response() {
        let root = root("muldex-daemon-transport-bad-payload-kind");
        let transport = FileCommandTransport::new(&root);
        let mut host = sample_host();

        let envelope = DaemonCommandEnvelope {
            schema_version: "daemon-envelope-v1".to_string(),
            command_id: "cmd-bad-payload-kind".to_string(),
            session_id: Some("session-1".to_string()),
            command_name: "apply_command".to_string(),
            payload_kind: "NotRuntimeCommand".to_string(),
            payload_json: "{}".to_string(),
            created_at_ms: 1,
        };

        transport.write_command(&envelope).expect("write command");
        let responses = transport.process_commands(&mut host).expect("process command");

        assert_eq!(responses.len(), 1);
        assert!(!responses[0].ok);
        assert_eq!(responses[0].payload_kind, "Error");
        assert!(responses[0]
            .error
            .as_deref()
            .expect("error")
            .contains("unsupported daemon command payload_kind"));
        assert!(!transport
            .commands_dir()
            .join("cmd-bad-payload-kind.json")
            .exists());
        assert!(transport
            .archive_dir()
            .join("cmd-bad-payload-kind.json")
            .exists());
        assert!(transport
            .responses_dir()
            .join("cmd-bad-payload-kind.json")
            .exists());
        assert_eq!(host.get_state("session-1").expect("state").cycle_index, 0);

        let _ = fs::remove_dir_all(&root);
    }
}
