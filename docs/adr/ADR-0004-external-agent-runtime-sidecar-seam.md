# ADR-0004: External Agent Runtime Through a Sidecar Seam

## Status

Accepted

## Context

`muldex` may benefit from external runtimes such as Agently for:

- planning
- evaluator or reviser policies
- workflow orchestration
- future multi-agent strategies

However, these integrations must not take over local execution authority or persistence.

## Decision

External agent runtimes integrate through a narrow sidecar or subprocess seam.

They may provide:

- planner-style outputs
- structured continuation advice
- workflow recommendations

They do not directly own:

- sandbox authority
- session persistence
- local tool execution authority
- runtime host state

## Consequences

Positive:

- external orchestration can be adopted without yielding kernel ownership
- failures or replacement of sidecars are bounded
- Rust remains the trusted runtime surface

Negative:

- conversion layers and adapter code are required
- some advanced orchestration features may take longer to integrate cleanly

## Rejected alternatives

### Make Agently or another external runtime the main kernel

Rejected because it would collapse the kernel authority boundary and complicate sandbox and approval compatibility.

### Avoid all external runtime integration

Rejected because bounded sidecar integration is still useful for planning and future orchestration experiments.
