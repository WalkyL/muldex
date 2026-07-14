# Codex-style TUI demo plan

## Goal

Deliver one **Windows x64 demo-quality interactive shell** that feels structurally close to Codex:

- stable full-screen TUI
- fixed top status bar
- scrollable transcript area
- right status panel
- bottom multi-line composer
- slash-command hint area
- no screen tearing / overlapping redraw artifacts

Non-goals for this slice:

- Linux/macOS parity
- Windows ARM64 build
- daemon/network parity polishing
- full Codex feature parity
- architecture changes outside CLI shell rendering/input boundary

## Architecture freeze

Low-tier coding work must stay inside this architecture. Do **not** redesign during implementation.

### Existing domain/runtime kept as-is

Keep these existing concerns in `crates/muldex-cli/src/main.rs`:

- session persistence
- runtime stepping
- slash command semantics
- provider/config mutation
- approval/model state mutation
- prompt handling business rules

### New TUI boundary

Introduce `crates/muldex-cli/src/interactive_tui/` with pure/presentational modules only:

1. `mod.rs`
   - public TUI entry helpers
   - orchestration between renderer + view model + terminal session

2. `view_model.rs`
   - converts runtime/shell/prompt/search/completion state into render-safe view model
   - no terminal IO
   - deterministic / test-first

3. `theme.rs`
   - all colors, labels, section titles, badges
   - no business logic

4. `layout.rs`
   - ratatui pane split helpers
   - constants for minimum panel sizes

5. `render.rs`
   - ratatui widgets/frame drawing only
   - no mutation of runtime state

6. `terminal.rs`
   - alternate screen enter/leave
   - raw mode lifecycle guard
   - terminal bootstrap/shutdown

### Allowed wiring changes in `main.rs`

Only these changes are allowed in `main.rs`:

- call new TUI renderer in TTY mode
- keep plain shell fallback for non-TTY mode
- replace current manual ANSI redraw path
- map existing shell state into TUI view model

### Forbidden during coding

- no runtime protocol redesign
- no daemon protocol changes
- no provider config schema changes
- no session persistence format changes unless absolutely required for tests
- no broad refactor of unrelated CLI commands

## UX target for demo

### Layout

Top bar:

- product name `muldex`
- session id (truncated)
- phase
- model
- approval mode
- cycle index

Main area split:

- left: transcript
- right: status/inspector

Bottom area:

- slash hint strip when active
- composer box with cursor support

### Transcript rules

- show newest messages at bottom
- distinguish `system`, `user`, `assistant`
- keep only recent N visible rows in the viewport model
- never print directly via `println!` during TUI mode except terminal bootstrap failure paths

### Right panel rules

Show compact, human-reviewable facts only:

- phase
- objective
- last outcome
- pending approval flag
- compact count
- resume count
- provider/model summary

### Input rules

- `Enter` submit single-line prompt
- `Ctrl+J` insert newline
- `Tab` slash completion advance
- `Esc` close completion / clear search / clear buffer per existing behavior
- `Ctrl+R` history search
- `Ctrl+C` / `Ctrl+D` exit shell

## Test strategy (TDD)

Implementation must proceed in thin vertical slices. Every slice starts with tests.

### Slice 1 — view model foundation

Implementation items:

- add ratatui dependency
- create `interactive_tui::view_model`
- define `ShellViewModel`, `TopBarViewModel`, `TranscriptItemViewModel`, `StatusPanelViewModel`, `ComposerViewModel`
- add builder function from existing shell/runtime snapshot inputs

Tests first:

- top bar includes phase/model/approval/cycle/session summary
- transcript maps message roles to stable labels
- status panel exposes objective/outcome/provider fields
- slash hint visibility only when completion visible and hints non-empty

Acceptance criteria:

- `cargo test -p muldex-cli view_model` passes
- zero terminal IO inside view model module
- no behavior changes to plain-shell mode

### Slice 2 — ratatui static frame renderer

Implementation items:

- add `layout.rs`, `theme.rs`, `render.rs`
- render top bar, transcript block, right panel, composer block
- use ratatui test backend for snapshot-like assertions

Tests first:

- frame contains top titles and section labels
- transcript pane renders role markers
- status pane renders approval/model labels
- composer renders prompt content and cursor line prefix

Acceptance criteria:

- renderer tests pass on `TestBackend`
- no ANSI clear-screen sequences remain in TUI render path
- frame draws deterministically for fixed input

### Slice 3 — terminal session lifecycle

Implementation items:

- add `terminal.rs`
- alternate screen + raw mode guard
- restore terminal on drop/error path

Tests first:

- guard creation path is abstracted behind testable trait or helper
- failure paths restore state through guard drop semantics

Acceptance criteria:

- no leaked raw mode after abnormal return in tests
- TTY mode path owns terminal lifecycle in one place only

### Slice 4 — wire TUI into interactive shell loop

Implementation items:

- replace manual `render_interactive_shell_view` / prompt redraw path for TTY mode
- keep non-TTY plain mode unchanged
- redraw through ratatui after state transitions and key handling

Tests first:

- scripted-key session can enter prompt text and exit without panic
- `/help` and `/status` still mutate output/session state as before
- non-TTY path still prints plain shell banner/header

Acceptance criteria:

- `cargo test -p muldex-cli interactive` passes
- `muldex.exe` launches full-screen TUI in TTY mode
- non-TTY execution still exits cleanly on EOF

### Slice 5 — demo polish

Implementation items:

- improve labels/badges/colors
- transcript truncation and right-panel wording
- empty states for no provider/no messages

Tests first:

- empty transcript renders placeholder
- missing provider renders explicit not-configured state
- long session id/objective truncation stays within layout

Acceptance criteria:

- Windows x64 release binary shows stable demo UI
- human reviewer can identify session/model/approval/phase/transcript/composer immediately
- no overlapping text during prompt typing, history search, slash completion

## Final acceptance checklist

All items must pass before todo closes:

1. `cargo test -p muldex-cli`
2. `cargo build --release --target x86_64-pc-windows-msvc`
3. Interactive shell opens as full-screen TUI in real terminal
4. `/help`, `/status`, `/approval`, `/model`, `/exit` still work
5. plain non-TTY shell still works for scripted/demo execution
6. no manual ANSI whole-screen redraw function remains active in TTY path
7. reviewer sign-off: “demo-quality Codex-like layout achieved for Windows x64 scope”

## Task execution protocol for delegated coding

Low-tier coding agent must:

1. work slice by slice in order
2. write tests first in each slice
3. never change architecture without escalating
4. report changed files, tests added, tests run, and remaining failures after each slice

Validation agent must:

1. review diff against this architecture doc
2. run/inspect acceptance checklist
3. report blocking gaps only
