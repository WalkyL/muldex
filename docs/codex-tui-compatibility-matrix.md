# Codex TUI Compatibility Matrix

## Purpose

Track how close `muldex` is to the operator experience of the upstream Codex TUI.

This document is intentionally practical.
It distinguishes between:

- behavior that is already compatible enough for migration
- behavior that is only partially compatible
- behavior that is still missing

## Status legend

- `Implemented` = available now in the default `muldex` shell
- `Partial` = available in a narrower or lower-fidelity form
- `Missing` = not yet implemented

## Entry and shell model

| Area | Codex-style expectation | `muldex` status | Notes |
| --- | --- | --- | --- |
| Default entry | Running `muldex` should open an interactive shell | Implemented | Root command enters the interactive shell when no subcommand is provided |
| Prompt-on-launch | `muldex "prompt"` should seed the first turn | Implemented | Initial prompt is executed on shell start |
| Session shell | Shell should preserve a session-oriented workflow | Implemented | Session header, persisted messages, session restore, and active runtime state exist |
| Stable terminal view | Shell should behave like a terminal app rather than a one-shot CLI | Partial | TTY mode uses stable redraw; non-TTY mode stays line-oriented for scripts and smoke tests |

## Session continuity

| Area | Codex-style expectation | `muldex` status | Notes |
| --- | --- | --- | --- |
| Resume most recent session | Resume prior work without rebuilding context manually | Implemented | `/resume` restores active shell state |
| Resume named session | Resume a specific previous session | Implemented | `/resume <id>` works |
| List sessions | Show resumable sessions | Implemented | `/sessions` works; plain and TTY paths both surface session information |
| Start fresh session | Begin a new independent shell session | Implemented | `/new` works |
| Session persistence | Restore runtime state, messages, and shell state | Implemented | Store persists runtime state, message log, prompt history, and shell controls |

## Slash command compatibility

| Area | Codex-style expectation | `muldex` status | Notes |
| --- | --- | --- | --- |
| `/help` | Show shell help | Implemented | Basic command list shown |
| `/status` | Show current runtime/session state | Implemented | Surfaces runtime-backed model, approval, compaction, phase, and counters |
| `/model` | Show/set active model | Implemented | Backed by runtime continuation state |
| `/approval` | Show/set approval mode | Implemented | Backed by runtime safety state |
| `/compact` | Request compaction | Implemented | Backed by runtime post-compaction state |
| `/sessions` | List resumable sessions | Implemented | Session-aware and persisted |
| `/resume` | Resume active or named session | Implemented | Active and named session restore both work |
| `/new` | Create fresh interactive session | Implemented | Creates a new persisted shell session |
| Broader Codex slash catalog | Parity with the larger upstream slash surface | Missing | Only the currently implemented subset exists |

## Composer and editing behavior

| Area | Codex-style expectation | `muldex` status | Notes |
| --- | --- | --- | --- |
| Single-line editing | Insert, move cursor, delete | Implemented | Character insert, left/right, backspace |
| Word editing | Word motion and delete word | Implemented | `Alt+Left`, `Alt+Right`, `Ctrl+W` |
| Home/End | Jump to line boundaries | Implemented | Supported in raw-mode shell |
| Clear line | Fast line clear | Implemented | `Ctrl+U` |
| Multiline compose | Compose multi-line prompt before submit | Partial | `Ctrl+J` inserts newline; no richer multiline composer widget yet |
| History recall | Navigate prompt history | Implemented | Non-slash `Up/Down` restores history and draft |
| Reverse history search | Search history from current draft | Partial | `Ctrl+R` supports incremental reverse search with visible search state |
| Rich search UI | Full interactive search widget | Missing | Current search is textual and inline rather than a dedicated widget |

## Slash picker and command discovery

| Area | Codex-style expectation | `muldex` status | Notes |
| --- | --- | --- | --- |
| Slash hints | Show matching slash commands while typing | Implemented | First-line slash prefix filters command catalog |
| Keyboard navigation | Navigate slash candidates from keyboard | Implemented | `Up/Down` move active slash candidate when slash mode is active |
| Apply selected candidate | Apply highlighted candidate without manual full typing | Implemented | `Tab` and picker-aware `Enter` both apply selected item |
| Dismiss picker | Close picker without destroying input | Implemented | First `Esc` hides picker, second clears input |
| Rich picker layout | Modal/popup style picker with stronger affordances | Partial | Current picker is inline text with explicit active row marker |

## Runtime and safety linkage

| Area | Codex-style expectation | `muldex` status | Notes |
| --- | --- | --- | --- |
| Frontend controls affect real runtime state | UI actions should change runtime state, not shell-only placeholders | Implemented | `/model`, `/approval`, `/compact` all affect runtime-backed fields |
| Read-only safety gating | Read-only paths should be enforced | Implemented | Client command gating and typed error projection already exist |
| Approval visibility | Operator should see approval policy state clearly | Implemented | `/status` shows runtime-backed approval state |
| Compaction visibility | Operator should see compaction request state clearly | Implemented | `/status` shows runtime-backed compaction state |

## Rendering and terminal behavior

| Area | Codex-style expectation | `muldex` status | Notes |
| --- | --- | --- | --- |
| Stable redraw in interactive terminal | Screen behaves like an interactive terminal application | Partial | TTY redraw path exists; not yet a full pane/layout framework |
| Non-interactive safety | Piped/scripted usage should stay predictable | Implemented | Plain output path retained for non-TTY and smoke usage |
| PTY-backed automation | CI should exercise true terminal behavior | Partial | A manual PTY/ConPTY smoke hook exists but remains ignored due environment sensitivity |

## Major missing or partial areas

These are the main gaps that still separate the current shell from a stronger Codex-like TUI:

1. richer pane/layout system rather than hand-rolled redraws
2. larger slash-command surface
3. stronger PTY/ConPTY automation for real terminal input paths
4. richer reverse-history-search and picker UI
5. broader shortcut parity beyond current high-frequency editing and picker flows

## Current migration summary

`muldex` is now beyond a debug/admin CLI.
It has a real default shell, persisted sessions, a runtime-backed slash-command layer, a multi-line composer path, slash picker navigation, history recall, and visible reverse history search.

That makes it a plausible migration shell for focused operator testing, even though it is not yet a full Codex TUI replacement.
