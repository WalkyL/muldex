# ADR-0013: Default Interactive Shell and Codex Migration Surface

## Status

Accepted

## Context

`muldex` started with a runtime- and governance-focused CLI surface.

That shape was useful for early daemon, continuity, transport, and contract work, but it is not the operator shape we ultimately want.

The project requirement is not just to preserve Codex semantics internally.
It must also preserve enough of Codex's operator habits that a user can move into `muldex` without learning a completely different shell model.

At this point `muldex` already has:

- a default interactive shell entrypoint
- persisted interactive shell sessions
- a Codex-style slash command slice
- shell-side history recall and reverse search
- a stable redraw path for TTY mode
- a plain path for scripted and non-TTY use

This is now substantial enough to fix the front-surface direction as an architecture decision rather than leave it implicit.

## Decision

`muldex` will use the default interactive shell as the primary operator surface.

The intended operator experience is:

- invoking `muldex` with no subcommand enters the interactive shell
- shell interaction should stay as close as practical to Codex TUI operator habits
- runtime, daemon, continuity, and debug subcommands remain available, but they are not the primary migration surface

## What this means

### Primary surface

- default interactive shell
- session continuity through `/new`, `/sessions`, and `/resume [id]`
- Codex-style slash compatibility as the first migration mechanism
- shell editing and search behavior should continue evolving toward practical operator familiarity

### Secondary surface

- daemon commands
- client commands
- snapshot import/export commands
- one-shot debug and validation commands

These remain important for development, validation, and infrastructure workflows, but they are not the first-shell story.

## Why this was chosen

- users already know the Codex shell rhythm
- migration cost is reduced when the shell entrypoint and interaction style are familiar
- current `muldex` runtime work is now rich enough to support a real operator shell instead of only admin commands
- keeping a compatibility-oriented shell does not prevent deeper runtime changes underneath

## Consequences

Positive:

- the product direction is clearer
- future shell changes can be judged against a concrete migration target
- slash command, history, picker, and shell rendering work are now first-class architecture concerns rather than optional polish

Negative:

- front-end UX debt becomes more visible and must be managed deliberately
- shell behavior and terminal performance now matter as much as daemon and protocol correctness for trial readiness

## Rejected alternatives

### Keep the admin/debug command surface as the main user interface

Rejected because it would maximize migration friction and fail the requirement to preserve Codex operator habits where practical.

### Build a fully novel shell interaction model first and map Codex later

Rejected because it would increase divergence too early and make compatibility harder to recover later.
