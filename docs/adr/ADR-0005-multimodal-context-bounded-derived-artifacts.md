# ADR-0005: Multimodal Context Uses Bounded Derived Artifacts

## Status

Accepted

## Context

`muldex` is intended to support audio, video, and hyperframe-aware workflows while preserving a Codex-like operator rhythm.

The main risk is letting raw media payloads leak into model context and orchestration logic in an unbounded way.

## Decision

Multimodal context enters `muldex` through bounded derived artifacts and summaries rather than raw media injection.

The runtime may accept media assets, but orchestration and model-facing layers reason over:

- transcripts
- subtitle tracks
- keyframes
- segment summaries
- evidence summaries
- hyperframes
- references to managed artifacts

## Consequences

Positive:

- multimodal context remains inspectable and bounded
- orchestration and routing stay compatible with explicit capability contracts
- long-form media can be incorporated without rewriting continuation logic around opaque blobs

Negative:

- media ingestion requires preprocessing stages before orchestration can use the result
- end-to-end workflows depend on artifact derivation quality

## Rejected alternatives

### Inject raw audio or video directly into orchestration context

Rejected because it makes context control weak, inspection difficult, and long-task governance harder.

### Treat multimodal support as a later bolt-on with no protocol boundary

Rejected because it would force architectural churn once multimodal workflows become real requirements.
