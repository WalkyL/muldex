# ADR-0008: Resume, Import, and Export Contract

## Status

Accepted

## Context

`muldex` already has several related continuity needs:

- restore `RuntimeHost` and session state from snapshots
- continue work after process restart
- import bounded state from Codex snapshots
- eventually export runtime state for replay, transfer, or debugging

Without a coherent contract, these could become separate ad hoc features.

## Decision

Treat resume, import, and export as one structured continuity contract with distinct entry points.

The intended separation is:

- `resume`: restore `muldex`-native runtime state and keep working
- `import`: translate external bounded state such as Codex snapshots into `muldex` runtime inputs or snapshots
- `export`: emit `muldex`-native runtime state or reports for later replay, debugging, or transfer

Current implementation note:

- `muldex-runtime` now includes a continuity module with explicit native host resume/export helpers and external snapshot import into `RuntimeState`
- the current implementation covers host/session export, host resume, and Codex snapshot import into runtime state
- richer replay and export surfaces can extend this contract later without collapsing the separation

## Consequences

Positive:

- continuity semantics stay consistent across local resume and external import
- the project avoids conflating upstream snapshot translation with native runtime restore
- debugging and replay can build on the same export boundary later

Negative:

- more naming and interface discipline is required early
- import/export versioning concerns show up earlier than in a purely internal design

## Rejected alternatives

### Treat resume and import as the same operation

Rejected because native restore and external translation have different trust and fidelity assumptions.

### Delay export semantics until replay is implemented

Rejected because export shape affects persistence, debugging, and host integration choices now.
