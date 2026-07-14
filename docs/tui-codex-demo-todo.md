# TUI Codex demo TODO

Reference architecture/spec: `docs/tui-codex-demo-plan.md`

## Phase 1 — View model foundation

### Tasks
- [ ] Add `ratatui` dependency to `crates/muldex-cli/Cargo.toml`
- [ ] Create `crates/muldex-cli/src/interactive_tui/mod.rs`
- [ ] Create `crates/muldex-cli/src/interactive_tui/view_model.rs`
- [ ] Define render-safe view-model structs for top bar / transcript / status panel / composer / slash hints
- [ ] Add builder function from current interactive shell state into TUI view model
- [ ] Add focused unit tests for field mapping and visibility rules

### Acceptance
- [ ] `cargo test -p muldex-cli view_model` passes
- [ ] No terminal IO in view model code
- [ ] Existing plain shell behavior unchanged

## Phase 2 — Static ratatui frame renderer

### Tasks
- [ ] Create `theme.rs`
- [ ] Create `layout.rs`
- [ ] Create `render.rs`
- [ ] Render fixed top bar
- [ ] Render left transcript panel
- [ ] Render right status panel
- [ ] Render bottom composer block
- [ ] Render slash hint strip when active
- [ ] Add test-backend renderer tests

### Acceptance
- [ ] Renderer tests pass on deterministic frame output
- [ ] TUI path no longer depends on manual ANSI full-screen clears
- [ ] Transcript / status / composer visible in one frame

## Phase 3 — Terminal lifecycle wrapper

### Tasks
- [ ] Create `terminal.rs`
- [ ] Add alternate-screen enter/leave guard
- [ ] Add raw-mode lifecycle guard around TUI session
- [ ] Centralize terminal restore on normal and error exits
- [ ] Add tests for lifecycle abstractions where feasible

### Acceptance
- [ ] TTY lifecycle owned in one place
- [ ] No leaked raw mode after tested failure paths
- [ ] No terminal bootstrap logic scattered across `main.rs`

## Phase 4 — Wire TUI into shell loop

### Tasks
- [ ] Replace current TTY redraw path with ratatui draw path
- [ ] Keep non-TTY plain shell fallback
- [ ] Preserve current key handling semantics
- [ ] Preserve slash command semantics
- [ ] Preserve prompt submission behavior
- [ ] Add scripted-key tests for open/input/exit

### Acceptance
- [ ] `cargo test -p muldex-cli interactive` passes
- [ ] TTY mode launches full-screen TUI
- [ ] Non-TTY mode still works on EOF and scripted/demo runs

## Phase 5 — Demo polish

### Tasks
- [ ] Improve labels and badges for phase/model/approval
- [ ] Add empty-state transcript placeholder
- [ ] Add explicit not-configured provider state
- [ ] Truncate long session/objective strings safely
- [ ] Verify no overlap during slash completion
- [ ] Verify no overlap during history search
- [ ] Verify multi-line composer stays stable

### Acceptance
- [ ] Windows x64 release binary is demo-quality
- [ ] Human can instantly identify top bar / transcript / status / composer
- [ ] No visible text collisions in normal interactive use

## Final gate
- [ ] `cargo test -p muldex-cli`
- [ ] `cargo build --release --target x86_64-pc-windows-msvc`
- [ ] Real-terminal manual demo works
- [ ] `/help` `/status` `/approval` `/model` `/exit` verified
- [ ] Reviewer sign-off completed
