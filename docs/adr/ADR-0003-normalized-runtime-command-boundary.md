# ADR-0003: Normalized Runtime Command Boundary

## Status

Accepted

## Context

The runtime needs to handle different kinds of inputs:

- external events
- single continuation decisions
- bounded drive sequences
- scripted event/decision interleaving
- resume-after-event flows

If callers invoke different helper functions directly, the runtime boundary becomes inconsistent and hard to extend.

## Decision

`muldex-runtime` normalizes runtime inputs through:

- `RuntimeCommand`
- `RuntimeCommandResult`
- `RuntimeDriver::apply_command(...)`

Lower-level helpers remain available, but the normalized command path is the preferred external entrypoint.

## Consequences

Positive:

- CLI, future daemon code, schedulers, and bridges can all speak one runtime input language
- the runtime can evolve its internals without forcing every caller to know which helper to invoke
- command logging and replay become easier later

Negative:

- one more abstraction layer exists above simple helper calls
- command/result enums must stay coherent as runtime features grow

## Rejected alternatives

### Let every caller use whichever runtime helper it wants

Rejected because that creates multiple competing runtime boundaries and makes later host/daemon work harder.

### Hide all helper functions immediately

Rejected because the current project still benefits from keeping lower-level helpers visible for testing and experimentation.
