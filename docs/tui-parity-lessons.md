# TUI Parity Retrospective — Lessons Learned

Distilled from closing the 20 parity gaps (`docs/codex-tui-parity-gap-analysis.md`)
to replicate the Codex CLI TUI 1:1 in `muldex`. Kept as institutional memory; the
gap plan itself has been retired.

## 1. Windows console: VT paints, Win32 controls

crossterm's VT escapes are sufficient for cell painting (truecolor, syntax
highlighting). But cursor **shape** (block vs bar/underline), scrollback purge,
and live resize detection require native Win32 APIs. The right seam is a custom
ratatui `Backend` (`WinConBackend`) that delegates painting to `CrosstermBackend`
while owning cursor style (`SetConsoleCursorInfo`), scrollback (`ESC[3J`), and
size/resize (`GetConsoleScreenBufferInfo` `srWindow`) via `windows-sys`. Keep the
screen-buffer `HANDLE` in a `Cell` so `suspend`/`resume` (external editor) can
swap buffers without rebuilding the backend.

## 2. Raw input: read `ReadConsoleInputW`, translate faithfully

`event::read` is fine, but full fidelity (IME, Shift/CAPS, Ctrl+letter) needs
`ReadConsoleInputW`. Translate using the record's `uChar` for the real character,
fall back to VK for non-character keys, and normalize Ctrl+letter to
`Char('a') | CONTROL` so existing keymaps keep matching. Surface `WINDOW_BUFFER_SIZE_EVENT`
as a `Resize` so the frame re-renders.

## 3. Vim mode needs a kill/yank ring + visible mode

A Vim layer is unusable without (a) a yank/delete ring and (b) a visible mode
indicator (`-- NORMAL --` in the composer title, block cursor). Ship it behind an
env gate (`MULDEX_VIM=on`) so the default UX is unchanged and the shipping binary
is unaffected.

## 4. Approval must happen before side effects

The approval modal must gate tool execution, be keyboard-drivable
(`a`/`d`/`c`/`Esc`), and degrade gracefully (`MULDEX_APPROVAL_MODE=off`).

## 5. Streaming markdown is incremental by necessity

A live transcript requires incremental parse of the Responses API SSE
(`response.output_text.delta`) into `UiEvent`s. Render markdown spans per line
(`LinesWithEndings`) to avoid embedded-newline breakage in syntax highlighting.

## 6. External editor: suspend then resume the alternate screen

Launching an editor over the TUI corrupts state unless raw mode is dropped and the
original screen buffer restored first (`suspend`), then re-entered after
(`resume`). Same pattern for any subshell takeover.

## 7. Adaptive theming is env-driven, not assumed

Detect terminal background via an env probe (`MULDEX_PROBE_BG`) plus a
`MULDEX_THEME` (light/dark/auto) switch rather than hardcoding colors.

## 8. Release build: self-hosted + podman saves GitHub minutes

Windows + Linux release compilation belongs on the self-hosted build host
(192.168.1.52). Linux cross-builds run in a podman container
(`wsl -d podman-machine-default podman run rustembedded/cross:<target>`) so the
heavy lifting never consumes GitHub-hosted minutes. macOS still needs
GitHub-hosted runners (no easy cross path). The `release-build.yml` Linux step
previously depended on `product-manager-agent` + `config/products.example.json`,
neither of which existed; it now calls podman directly.

## 9. Provider/ReAct wiring

The Responses API streaming maps cleanly to a `UiEvent` stream. Surface
token usage and rate limits in the status panel and footer; the provider layer
(`responses_provider`) parses `Usage`/`RateLimit` into view models.

## 10. Testability: a PTY smoke test catches what units miss

A `portable-pty` smoke test catches regressions like "typing `hi` exits the UI"
that unit tests cannot. Keep it as an ignored test gated on the real router
(`--test e2e_router -- --ignored`).
