# Client Contract v1

## Purpose

Freeze the current read-mostly client JSON surface so future desktop, remote, or mobile clients have a stable reference.

This document describes the current `client-view-v1` contract.

## Contract metadata

Current client-facing views expose:

- `schema_version`
- read-only capability allowlist

Current version string:

```json
"client-view-v1"
```

## Read-only enforcement note

Current CLI client path defaults to read-only access mode.

At present, read-only mode allows only read-safe command kinds such as `status`.
Mutating command kinds such as `advance-sample` require explicitly widened access mode.

## Example: typed client command view

```json
{
  "contract": {
    "schema_version": "client-view-v1",
    "read_only_capabilities": [
      "DaemonStatus",
      "SessionList",
      "SessionInspect",
      "SessionExport",
      "ResponseRead"
    ]
  },
  "session_id": "sample-session",
  "access_mode": "ReadOnly",
  "command": "Status"
}
```

## Example: read-only deny case

If client tries to send a mutating command such as `AdvanceSample` while still in `ReadOnly` mode, current CLI contract rejects it before writing a daemon command envelope.

Illustrative error text:

```text
client access mode ReadOnly does not allow command kind advance-sample
```

## Example: daemon status view

```json
{
  "contract": {
    "schema_version": "client-view-v1",
    "read_only_capabilities": [
      "DaemonStatus",
      "SessionList",
      "SessionInspect",
      "SessionExport",
      "ResponseRead"
    ]
  },
  "snapshot_path": "D:\\workspaces\\muldex\\tmp-daemon.json",
  "daemon_status": "Running",
  "session_count": 1,
  "stale_status": "fresh",
  "heartbeat_age_ms": 12
}
```

## Example: session list view

```json
{
  "contract": {
    "schema_version": "client-view-v1",
    "read_only_capabilities": [
      "DaemonStatus",
      "SessionList",
      "SessionInspect",
      "SessionExport",
      "ResponseRead"
    ]
  },
  "session_count": 1,
  "sessions": [
    {
      "session_id": "sample-session",
      "thread_id": "thread-1",
      "cycle_index": 3,
      "phase": "Running"
    }
  ]
}
```

## Example: session inspect view (raw report)

```json
{
  "session_id": "sample-session",
  "thread_id": "thread-1",
  "cycle_index": 3,
  "phase": "Running",
  "report": {
    "Raw": {
      "run_id": "run:thread-1",
      "thread_id": "thread-1",
      "objective": "continue task",
      "execution_mode": "Interactive",
      "outcome": "InProgress",
      "rationale": "continue under harness policy",
      "cycle_summary": {
        "cycle_id": "cycle-3",
        "summary": "continue under harness policy",
        "completed_steps_delta": 0,
        "state_changes": ["NewConfirmedFinding"],
        "checkpoint_created": false,
        "approval_request_id": null,
        "pending_interrupt_count": 0
      },
      "generated_at_ms": null
    }
  }
}
```

## Example: session inspect view (compressed report)

```json
{
  "session_id": "sample-session",
  "thread_id": "thread-1",
  "cycle_index": 3,
  "phase": "Running",
  "report": {
    "Compressed": {
      "run_id": "run:thread-1",
      "thread_id": "thread-1",
      "rationale": "continue under harness policy",
      "compressed_cycle_summary": {
        "cycle_id": "cycle-3",
        "retention_class": "MayStubIfUnchanged",
        "summary": null,
        "stub": {
          "source_id": "cycle-2",
          "same_hash": "abc123",
          "unchanged_since": "cycle-2"
        }
      }
    }
  }
}
```

## Example: client response view

```json
{
  "contract": {
    "schema_version": "client-view-v1",
    "read_only_capabilities": [
      "DaemonStatus",
      "SessionList",
      "SessionInspect",
      "SessionExport",
      "ResponseRead"
    ]
  },
  "command_id": "cmd-1",
  "ok": true,
  "payload_kind": "RuntimeCommandResult",
  "payload": {
    "Step": {
      "phase": "Running",
      "cycle_index": 3,
      "outcome": "InProgress"
    }
  },
  "payload_json": "{\"Step\":{...}}",
  "error": null
}
```

## Example: daemon command envelope

```json
{
  "schema_version": "daemon-envelope-v1",
  "command_id": "cmd-1",
  "session_id": "sample-session",
  "command_name": "apply_command",
  "payload_kind": "RuntimeCommand",
  "payload_json": "{\"Decision\":{...}}",
  "created_at_ms": 0
}
```

## Daemon envelope validation

Current daemon file transport validates command envelopes before execution.

Required checks:

- `schema_version` must equal `daemon-envelope-v1`
- `payload_kind` must equal `RuntimeCommand`
- `session_id` must be present for executable commands

Validation failure behavior:

- command is archived
- response is written with `ok: false`
- response `payload_kind` becomes `Error`
- host state is not mutated

Illustrative error shape:

```json
{
  "schema_version": "daemon-envelope-v1",
  "command_id": "cmd-1",
  "ok": false,
  "payload_kind": "Error",
  "payload_json": "",
  "error": "unsupported daemon command payload_kind: NotRuntimeCommand",
  "created_at_ms": 1
}
```

## Example: daemon response envelope

```json
{
  "schema_version": "daemon-envelope-v1",
  "command_id": "cmd-1",
  "ok": true,
  "payload_kind": "RuntimeCommandResult",
  "payload_json": "{\"Step\":{...}}",
  "error": null,
  "created_at_ms": 1
}
```

## Example: client response view error payload

```json
{
  "contract": {
    "schema_version": "client-view-v1",
    "read_only_capabilities": [
      "DaemonStatus",
      "SessionList",
      "SessionInspect",
      "SessionExport",
      "ResponseRead"
    ]
  },
  "command_id": "cmd-1",
  "ok": false,
  "payload_kind": "Error",
  "payload": {
    "Error": {
      "message": "unsupported daemon command payload_kind: NotRuntimeCommand"
    }
  },
  "payload_json": "",
  "error": "unsupported daemon command payload_kind: NotRuntimeCommand"
}
```

## Example: client response view contract mismatch payload

```json
{
  "contract": {
    "schema_version": "client-view-v1",
    "read_only_capabilities": [
      "DaemonStatus",
      "SessionList",
      "SessionInspect",
      "SessionExport",
      "ResponseRead"
    ]
  },
  "command_id": "cmd-1",
  "ok": true,
  "payload_kind": "UnexpectedKind",
  "payload": {
    "ContractMismatch": {
      "expected_kind": "RuntimeCommandResult",
      "actual_kind": "UnexpectedKind",
      "raw_json": "{\"ok\":true}"
    }
  },
  "payload_json": "{\"ok\":true}",
  "error": null
}
```

## Example: client command receipt view

```json
{
  "contract": {
    "schema_version": "client-view-v1",
    "read_only_capabilities": [
      "DaemonStatus",
      "SessionList",
      "SessionInspect",
      "SessionExport",
      "ResponseRead"
    ]
  },
  "command_id": "cmd-1",
  "session_id": "sample-session",
  "command_name": "apply_command",
  "command_path": "D:\\workspaces\\muldex\\tmp\\daemon\\commands\\cmd-1.json"
}
```

## Stability note

Current contract is appropriate for:

- read-mostly client inspection
- mobile-safe polling clients
- remote dashboards

It is not yet a full interactive attach protocol.
