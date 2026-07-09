# ADR-0015: Pluggable External Session Importers

## Status

Accepted

## Context

`muldex` already supports bounded import from Codex snapshots through the continuity boundary.

The next requirement is broader:

- import Claude Code sessions
- import Codex sessions
- import OpenCode sessions

At the same time, the import path must not bloat the runtime kernel or force the kernel to understand every external transcript format directly.

The project also wants memory use to stay constrained during import.

## Decision

External session import will use a pluggable importer layer outside the runtime kernel.

The kernel should only consume normalized `muldex` continuity or runtime structures.

## Structure

### Importer layer

Importer plugins or adapters own:

- source-specific file discovery
- source-specific JSON or transcript parsing
- source-specific compatibility heuristics
- bounded extraction of only the state needed for normalization

Expected initial source families:

- Claude Code
- Codex
- OpenCode

### Normalization layer

Importers should normalize into a compact common intermediate shape before entering runtime continuity APIs.

The normalization contract should prefer:

- session identity
- thread or turn identity when available
- active model or provider metadata when available
- bounded objective, constraints, progress, approval, and checkpoint signals
- bounded recent message summaries rather than full raw replay by default

### Runtime continuity layer

`muldex-runtime` should continue to accept only normalized imported state and should not become the home of source-specific parser logic.

## Memory rule

Import should minimize peak memory use.

That means:

- do not require loading full raw transcripts into long-lived runtime memory by default
- normalize early
- keep full raw source replay as an optional diagnostic path, not the standard import path

## Consequences

Positive:

- new external import sources can be added without rewriting the kernel
- memory use remains more controllable
- source-specific complexity stays outside the runtime core

Negative:

- importer plugin interfaces must be designed and versioned carefully
- some source features may be dropped if they do not fit the normalized bounded import contract

## Rejected alternatives

### Teach the runtime kernel every source format directly

Rejected because it would entangle kernel evolution with source-specific transcript schemas.

### Default to full raw transcript replay for every import

Rejected because it would increase memory use and weaken the bounded continuity contract.
