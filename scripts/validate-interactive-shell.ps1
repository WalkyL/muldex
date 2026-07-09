$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot

function Invoke-Step {
    param(
        [string]$Label,
        [string]$Command
    )

    "==> $Label"
    & powershell -NoProfile -Command $Command
    if (-not $?) {
        throw "step failed: $Label"
    }
}

Invoke-Step -Label 'muldex-cli unit and smoke tests' -Command 'cargo test -p muldex-cli'
Invoke-Step -Label 'repo-wide cargo test' -Command 'cargo test'

"validation.result: ok"
"validation.note: PTY/ConPTY manual validation remains documented separately"
