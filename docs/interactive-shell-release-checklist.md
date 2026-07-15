# Interactive Shell Release Checklist

## Purpose

Define concrete release-readiness gates for the current `muldex` interactive shell.

This checklist is not for long-range product vision.
It is for deciding whether the current shell is ready for:

- personal trial
- broader operator trial
- stronger replacement claims

## Release levels

### Level 1: Personal trial

Safe to use for focused internal testing by the primary developer or operator.

### Level 2: Broader operator trial

Safe to hand to additional operators who can follow documented validation steps.

### Level 3: Codex TUI replacement claim

Strong enough to position as a serious interactive replacement for the current Codex TUI workflow.

This level is intentionally harder and should not be claimed early.

## Level 1 checklist: personal trial

Required:

- [x] default `muldex` launch enters the interactive shell
- [x] shell supports persisted sessions
- [x] shell supports `/new`, `/sessions`, and `/resume [id]`
- [x] shell supports runtime-backed `/model`, `/approval`, and `/compact`
- [x] shell supports prompt history recall
- [x] shell supports reverse history search
- [x] shell supports slash picker hints and keyboard selection
- [x] shell supports multiline prompt composition
- [x] `cargo test -p muldex-cli` passes
- [x] `cargo test` passes
- [x] operator docs exist for shell usage and validation

Current level assessment:

- `muldex` satisfies the current Level 1 gate

## Level 2 checklist: broader operator trial

Required:

- [x] compatibility matrix exists
- [x] interactive shell usage guide exists
- [x] Windows Terminal performance note exists
- [x] scripted validation entrypoint exists
- [x] manual validation checklist exists
- [x] shell redraw path distinguishes prompt-only vs full-frame redraw in current implementation
- [x] shell key state machine has strong unit coverage
- [x] non-TTY plain path remains stable for scripted use
- [x] real PTY/ConPTY automation exists for stable startup and minimal slash-command interaction smoke
- [x] operator-managed LLM router configuration exists inside the shell
- [x] config file can define additional providers manually
- [ ] multi-operator feedback has been collected from actual trial usage
- [ ] Windows Terminal long-session behavior has been manually checked across longer transcripts and not just smoke flows

Release-infrastructure checks for broader trial preparation:

- [x] trial preparation script exists
- [x] release build strategy is documented
- [x] minimal cross-platform install script skeletons exist
- [x] GitHub release workflow skeleton is defined for the multi-platform build matrix
- [x] packaging scripts exist for Windows zip and Unix tarball release artifacts
- [x] packaged artifact verification script exists
- [ ] per-platform packaged release artifacts have been exercised in CI release builds
- [x] workflow skeleton includes GitHub Release publication step for tag builds
- [x] install and uninstall paths are defined per platform
- [ ] installer asks whether to install and use `llm-router`
- [ ] installer explains the OpenAI-compatible request shape and why `llm-router` is recommended as a compatibility layer
- [ ] GitHub Actions routing to the `192.168.1.52` Windows x64 build host and hosted Linux jobs is verified in a real release run

Current level assessment:

- `muldex` is close to Level 2 and usable for controlled broader trial
- the biggest remaining gaps are multi-operator feedback and formal release infrastructure

## Level 3 checklist: stronger Codex replacement claim

Required:

- [ ] broader slash-command compatibility, not just the current focused subset
- [ ] stronger pane/layout system rather than the current hand-rolled stable shell redraw
- [ ] richer terminal UX polish for picker/search/composer regions
- [ ] stronger real-terminal automation and performance measurement
- [ ] larger operator validation set with migration feedback
- [ ] explicit decision that current gaps are acceptable for replacement positioning

Current level assessment:

- `muldex` does not yet satisfy Level 3

## Current recommended claim

The current safe claim is:

- `muldex` has a real interactive shell suitable for focused internal and controlled broader operator trial
- `muldex` is not yet a full Codex TUI replacement

## Required checks before each broader-trial refresh

Before handing a new build to additional operators, run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate-interactive-shell.ps1
```

Then manually review:

- `docs/interactive-shell-validation.md`
- `docs/windows-terminal-performance.md`
- `docs/codex-tui-compatibility-matrix.md`

## Known gate that still matters

The main unresolved release-quality gaps now are:

- formal multi-platform release packaging and installer paths
- broader operator trial evidence
- longer-session Windows Terminal performance evidence

Until that improves, broader trial is still reasonable, but replacement-level confidence should stay constrained.
