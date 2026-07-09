# Interactive Shell Trial Handoff

## Purpose

Describe how to prepare and hand off the current `muldex` interactive shell for controlled operator trial.

This is not a full external release process.
It is a practical internal trial flow.

## One-command preparation

From the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\prepare-interactive-shell-trial.ps1
```

That flow currently does three things:

1. builds `muldex-cli`
2. runs interactive shell validation
3. writes a small trial summary file under `trial-artifacts/`

## Expected outputs

Current generated artifacts:

- `target\debug\muldex.exe`
- `trial-artifacts\interactive-shell-trial-summary.txt`

The summary file records:

- the binary path
- the operator docs to read
- the current trial readiness outcome

## Trial handoff package

When handing the shell to an operator for controlled trial, provide:

1. the binary path
2. the summary file
3. these docs:
   - `docs/interactive-shell-guide.md`
   - `docs/interactive-shell-validation.md`
   - `docs/interactive-shell-release-checklist.md`
   - `docs/windows-terminal-performance.md`
   - `docs/codex-tui-compatibility-matrix.md`

## Recommended operator instruction

Suggested instruction set for a trial operator:

1. read the shell guide first
2. use the validation doc to understand manual checks
3. run the binary directly in Windows Terminal
4. focus on:
   - shell startup feel
   - slash picker feel
   - prompt editing responsiveness
   - session resume behavior
   - reverse history search feel
   - whether long usage in Windows Terminal slows down noticeably

## Current release posture

The current shell is suitable for:

- focused internal testing
- controlled broader operator trial

It is not yet positioned as a full Codex TUI replacement.

See also:

- `docs/interactive-shell-release-checklist.md`

## Trial feedback topics

Ask operators to report specifically on:

- migration friction from Codex habits
- shell responsiveness in Windows Terminal
- slash command discoverability
- history recall and reverse search quality
- session continuity expectations that still feel missing
