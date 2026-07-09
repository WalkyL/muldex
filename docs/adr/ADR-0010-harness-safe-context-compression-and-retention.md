# ADR-0010: Harness-Safe Context Compression and Retention

## Status

Proposed

## Context

`muldex` must control long-task context growth.

Pressure sources include:

- repeated tool output
- repeated file reads
- repeated search results
- repeated cycle reports
- repeated imported continuity context

Context compression is therefore attractive.

But `muldex` is not only a chat shell. It uses explicit harness, approval, safety, checkpoint, artifact, and runtime-state contracts. If compression or repeated-context dedup touches those wrongly, token savings could damage the orchestration boundary itself.

## Proposed decision

Context compression and repeated-context dedup must be harness-safe by design.

The intended rules are:

1. harness-critical context is never semantically deduplicated
2. early dedup is exact-match only
3. dedup-eligible context is explicitly classified
4. dedup never removes identity, provenance, or referenceability

Current implementation note:

- initial partial implementation exists only at report layer
- current code supports exact-only cycle-summary dedup into reference-preserving stubs
- harness core, approval state, safety state, and runtime decision inputs remain outside the dedup path
- full retention-class propagation across runtime context is not implemented yet

## Retention classes

The preferred retention model is explicit classification.

Suggested classes:

- `MustKeepExact`
- `MayStubIfUnchanged`
- `MaySummarize`
- `DropIfStale`

### `MustKeepExact`

Use for:

- active objective
- approval state and approval decisions
- safety and sandbox state
- prohibition and harness constraints
- checkpoint identity
- artifact identity and references
- unresolved blockers currently affecting decisions
- evidence directly used by current cycle decisions

### `MayStubIfUnchanged`

Use for:

- repeated unchanged tool output
- repeated unchanged file reads
- repeated unchanged grep windows
- repeated unchanged imported summaries
- repeated unchanged cycle-report narration

### `MaySummarize`

Use for:

- long historical supporting context no longer needed verbatim
- historical non-critical report text
- large background notes whose identity is preserved elsewhere

### `DropIfStale`

Use for:

- stale non-actionable chatter
- repeated low-value narration not referenced by current state

## Forbidden dedup zones

The following zones must not be deduplicated semantically:

- harness core
- approval and escalation state
- current safety boundary
- active checkpoint and checkpoint references
- artifact identifiers and evidence references
- current unresolved blocker set
- any context item already cited by a live decision boundary

## Exact-only first policy

Initial repeated-context dedup should be exact only.

That means:

- exact hash match allowed
- stable artifact-id equality allowed
- stable checkpoint-id equality allowed
- semantic similarity alone not sufficient

This reduces risk while the runtime is still maturing.

## Stub rule

When dedup occurs, replacement must preserve referenceability.

Allowed stub fields should include at least:

- source id
- prior reference id
- stable hash
- unchanged-since marker

Example stub shape:

- `same_as: artifact://grep/17`
- `same_hash: abc123`
- `unchanged_since: cycle-8`

## Consequences

Positive:

- token and context savings can grow without silently corrupting harness behavior
- runtime reports and imported continuity can be compacted more safely
- compression policy becomes kernel-governed rather than prompt folklore

Negative:

- context classification work must happen explicitly
- early compression wins may be smaller because semantic dedup is constrained
- reporting and artifact plumbing must preserve more identity metadata

## Rejected alternatives

### Let semantic dedup operate over all repeated context

Rejected because it risks collapsing safety, approval, and checkpoint-critical information.

### Treat all repeated context as equally compressible

Rejected because `muldex` uses structured runtime and harness contracts that do not all tolerate loss equally.

### Optimize token savings before retention safety is explicit

Rejected because broken harness behavior costs more than saved tokens.
