# ADR-0011: Daemon Lease, Heartbeat, and Stale-Owner Recovery

## Status

Proposed

## Context

`muldex` now has accepted daemon ownership and local command transport boundaries.

Current daemon ownership is file-backed and single-owner in shape, but it does not yet answer the hardest operational question:

- what should happen when the owner disappears without releasing the lock?

Without a lease and stale-owner policy, a crashed or abandoned daemon can block legitimate recovery indefinitely.

## Proposed decision

Daemon ownership should evolve from simple lock presence to lease-based ownership with heartbeat and explicit takeover rules.

The intended model is:

1. active daemon periodically refreshes ownership metadata
2. lock and daemon state together provide liveness evidence
3. takeover is allowed only after stale-owner conditions are satisfied
4. takeover action is explicit and visible, not silent

Current implementation note:

- initial partial implementation now exists
- daemon lock metadata includes `last_heartbeat_ms`
- local ownership can refresh heartbeat and classify stale status from lock metadata
- `RuntimeDaemon` refreshes heartbeat during save and transport-processing lifecycle points
- takeover policy and forced recovery are not implemented yet

## Intended policy areas

### 1. Lease

Ownership metadata should have time semantics, not only existence semantics.

Candidate fields:

- owner pid
- acquired at
- last heartbeat at
- runtime root
- snapshot path

### 2. Stale detection

Daemon should be considered stale only when policy thresholds are exceeded.

Candidate checks:

- heartbeat older than threshold
- daemon state not updated within threshold
- optional process liveness check when platform support is acceptable

### 3. Takeover

Takeover should be explicit.

Candidate paths:

- read-only status report of stale state
- operator-approved forced takeover
- future safe auto-recovery path only after rules are trusted

### 4. Visibility

Operator and CLI should be able to inspect:

- current owner metadata
- heartbeat age
- stale classification result
- whether takeover would be allowed

## Consequences

Positive:

- daemon recovery becomes possible without manual lockfile deletion
- operator trust improves because takeover rules are explicit
- future long-running service mode gains a safer recovery path

Negative:

- heartbeat and lease policy add time-based complexity
- incorrect stale thresholds could either block recovery or allow unsafe takeover

## Rejected alternatives

### Keep lock existence as the only ownership rule

Rejected because it cannot distinguish healthy owner from abandoned lock.

### Allow silent takeover whenever lock exists and process seems gone

Rejected because ownership transfer should be visible and reviewable.

### Solve stale-owner recovery only at the process-manager layer

Rejected because `muldex` still needs runtime-native visibility into ownership health and takeover safety.
