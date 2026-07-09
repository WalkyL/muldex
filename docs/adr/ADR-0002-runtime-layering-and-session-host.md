# ADR-0002: Runtime Layering and Session Host

## Status

Accepted

## Context

As `muldex` evolved, the architecture stopped being only a protocol sketch. The code now has distinct responsibilities:

- policy and protocol logic
- stateful runtime transitions
- operator-facing CLI behavior
- multi-session in-memory hosting

Without explicit layering, these responsibilities would drift back into one another.

## Decision

`muldex` uses layered runtime architecture:

- `muldex-core`
  - protocol types
  - reasoning harness
  - upstream adapters
  - sidecar seams
- `muldex-runtime`
  - runtime state machine
  - driver object
  - command application
  - in-memory session host
- `muldex-cli`
  - command-line entrypoints
  - debugging and baseline demonstrations

Within `muldex-runtime`, multi-session coordination currently lives in an in-memory `RuntimeHost` that owns session-scoped `RuntimeDriver` instances.

## Consequences

Positive:

- policy and runtime state transitions remain separable
- multi-session hosting is possible without turning the CLI into the host
- the daemon-facing shell can grow from `RuntimeHost` without redesigning core policy contracts

Negative:

- more cross-crate types must stay aligned
- some helper conversions still exist at CLI boundaries until more runtime inputs are normalized

## Rejected alternatives

### Keep everything inside `muldex-cli`

Rejected because it would make long-running runtime semantics depend on command wiring.

### Build a daemon process before a host layer exists

Rejected because it would force process-level work before the runtime ownership model stabilized.
