# Interactive Shell Validation

## Purpose

Describe how to validate the current `muldex` interactive shell as a trial-ready operator surface.

This document separates:

- automated checks that should stay stable
- manual checks that still matter for real terminal behavior, especially on Windows Terminal

## Fast automated validation

From the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate-interactive-shell.ps1
```

That script currently runs:

1. `cargo test -p muldex-cli`
2. `cargo test`

It intentionally focuses on stable automated coverage rather than the still-environment-sensitive PTY path.

## What the automated path proves

Current automated coverage validates:

- default shell entry
- shell prompt seeding
- session persistence and resume flows
- runtime-backed slash commands
- slash picker filtering, navigation, and apply behavior
- prompt history recall and reverse history search state machines
- forced TTY render branch
- repo-wide regression safety

## Manual validation checklist

Manual validation still matters for real Windows terminal behavior.

### A. Default shell launch

```powershell
cargo run -p muldex-cli --
```

Confirm:

- shell opens directly without subcommand
- session header is visible
- prompt appears responsive

### B. Slash picker and picker dismissal

Inside the shell:

1. type `/`
2. confirm picker hints appear
3. use `Up` / `Down`
4. use `Tab`
5. use first `Esc` to hide picker
6. use second `Esc` to clear buffer

Confirm:

- active picker row is clearly marked
- `Tab` or picker-aware `Enter` applies the selected command
- `Esc` behaves in two stages

### C. Composer editing

Inside the shell verify:

- `Left` / `Right`
- `Home` / `End`
- `Backspace`
- `Alt+Left` / `Alt+Right`
- `Ctrl+W`
- `Ctrl+U`
- `Ctrl+J` for multiline insertion

Confirm:

- cursor movement remains responsive
- multiline prompt stays editable
- no visible lag spike from ordinary typing

### D. History recall and reverse search

Submit a few prompts, then verify:

- `Up` / `Down` restore history and then return to draft
- repeated identical prompt submissions do not flood recall order
- `Ctrl+R` starts reverse search
- repeated `Ctrl+R` cycles older matches
- typing while search is active refines the query
- `Backspace` widens the query
- `Esc` restores the original draft

### E. Runtime-backed slash commands

Inside the shell verify:

- `/model`
- `/model <name>`
- `/approval`
- `/approval on-request`
- `/compact`
- `/status`

Confirm:

- shell output reflects runtime-backed state changes
- `/status` shows model, approval, compaction, and phase changes consistently

### F. Session continuity

Inside the shell verify:

- `/new`
- `/sessions`
- `/resume`
- `/resume <id>`

Confirm:

- new session gets a new id
- session list shows resumable sessions
- resume restores messages, history, and runtime-linked shell state

## Windows Terminal specific focus

Because this project explicitly wants to avoid the long-session slowdown weakness seen in Codex-style shells on Windows Terminal, watch for these signs:

- typing latency increasing over time
- arrow-key movement becoming visibly sticky
- whole-screen repaint feeling slower after longer transcripts
- lag spikes while slash picker or reverse search is active

If any of those appear, capture:

- the sequence of actions that caused the slowdown
- whether the shell was in slash-picker mode, reverse-search mode, or plain editing mode
- whether the slowdown appears only in Windows Terminal or also in other terminals

See also:

- `docs/windows-terminal-performance.md`

## Current PTY limitation

There is still a manual PTY/ConPTY validation hook in the test suite, but it remains ignored by default because of environment sensitivity on Windows.

That means:

- core shell state machines are strongly tested
- real PTY path still benefits from manual operator validation

## Recommended trial sign-off rule

Treat the current interactive shell as acceptable for broader operator trial only when:

1. `scripts/validate-interactive-shell.ps1` passes
2. manual Windows Terminal checks above pass without obvious latency regression
3. session resume and runtime-backed slash commands behave as expected
