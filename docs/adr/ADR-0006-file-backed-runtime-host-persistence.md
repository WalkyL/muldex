# ADR-0006: File-Backed Runtime Host Persistence

## Status

Accepted

## Context

`muldex-runtime` now supports in-memory host and session snapshot export/import through structured Rust types.

That is enough to prove a persistence boundary exists, but it does not yet solve:

- process restart recovery
- CLI resume across invocations
- future daemon crash recovery

The next likely step is file-backed persistence around the existing snapshot boundary.

## Decision

Add file-backed persistence at the host snapshot layer rather than inventing a second persistence model.

The preferred shape is:

- runtime state remains defined by `RuntimeState`, `RuntimeDriver`, `RuntimeSessionSnapshot`, and `RuntimeHostSnapshot`
- persistence writes and reads these snapshot structures directly
- file persistence is an adapter around the existing data model, not a parallel runtime representation

## Consequences

Positive:

- resume and recovery stay aligned with the current in-memory model
- daemon work can build on the same snapshot boundary
- snapshot round trips remain testable without a live daemon

Negative:

- snapshot schema changes become more important to manage
- host-level persistence may need versioning sooner than the pure in-memory model did

## Rejected alternatives

### Introduce a separate daemon-only persistence schema first

Rejected because it would split the runtime state model before the file-backed shape is even validated.

### Delay persistence design until after the daemon exists

Rejected because daemon recovery and CLI resume depend directly on persistence semantics.
