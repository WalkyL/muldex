# Windows Terminal Performance Notes

## Purpose

Record the current performance posture of the `muldex` interactive shell on Windows Terminal and explain the redraw choices in the shell implementation.

This exists because Codex-style terminal shells often degrade in responsiveness over long sessions on Windows when they redraw too aggressively.

## Problem we are explicitly trying to avoid

One recurring weakness in terminal-centric agent shells is that they become progressively slower in Windows Terminal during long usage.

Common causes include:

- redrawing the entire screen for every character
- repeated layout rebuilds for trivial cursor edits
- writing large transcript regions on every small input change
- mixing terminal control sequences with excessive scrollback growth

## Current shell posture

`muldex` now distinguishes between:

- prompt-only redraws
- full shell-frame redraws

The intent is simple:

- high-frequency editing actions should stay cheap
- only structural UI changes should trigger a full redraw

## Current redraw model

### Prompt-only redraws

These stay on the cheaper path whenever possible:

- character insertion during normal editing
- left and right cursor movement
- home and end movement
- word motion
- delete-word
- clear-line
- ordinary history recall where only the composer line changes

These operations now prefer prompt-level repaint rather than forcing a full shell-frame rebuild.

### Full-frame redraws

These still justify a larger redraw because shell-visible regions change:

- slash picker open, close, or selection change
- slash completion application
- reverse history search activation or refinement
- reverse history search restore and exit
- shell status refresh
- runtime-affecting slash command flows after submission

## Why plain mode still exists

The shell intentionally keeps a non-TTY/plain path.

This is useful for:

- smoke tests
- scripted usage
- low-risk diagnostics
- avoiding terminal-control-sequence noise in environments that are not real terminals

That split also reduces the temptation to force every path through the more expensive interactive redraw model.

## Current operator guidance

If Windows Terminal becomes slow in a long session, check these first:

1. Prefer the default interactive shell only when you actually want the session UI
2. Use plain mode for scripted or repeated smoke work
3. Avoid mixing huge transcript growth with constant interactive editing if you only need one-shot command validation

Force plain mode:

```powershell
$env:MULDEX_FORCE_PLAIN_SHELL = "1"
cargo run -p muldex-cli --
```

Force the interactive render branch for diagnostics:

```powershell
$env:MULDEX_FORCE_TTY_RENDER = "1"
cargo run -p muldex-cli --
```

## Current limits

This is not yet a full performance solution.

Remaining limits:

- shell view still uses hand-rolled redraws rather than a dedicated diffing TUI framework
- PTY/ConPTY automation for real terminal behavior remains partially environment-sensitive on Windows
- long-session throughput has not yet been benchmarked formally against transcript size and redraw frequency

## Next likely performance steps

The most likely next improvements are:

1. track redraw intent more explicitly in more shell branches
2. avoid recomputing or reprinting regions that did not change
3. add real performance probes for long interactive sessions on Windows Terminal
4. benchmark transcript growth versus redraw latency before moving to a richer pane system
