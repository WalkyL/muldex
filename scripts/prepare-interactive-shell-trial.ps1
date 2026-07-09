$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$artifactRoot = Join-Path $repoRoot 'trial-artifacts'
$docRoot = Join-Path $repoRoot 'docs'

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

if (-not (Test-Path -LiteralPath $artifactRoot)) {
    New-Item -ItemType Directory -Path $artifactRoot | Out-Null
}

Invoke-Step -Label 'build muldex-cli' -Command 'cargo build -p muldex-cli'
Invoke-Step -Label 'interactive shell validation' -Command 'powershell -ExecutionPolicy Bypass -File .\scripts\validate-interactive-shell.ps1'

$binaryPath = Join-Path $repoRoot 'target\debug\muldex.exe'
$summaryPath = Join-Path $artifactRoot 'interactive-shell-trial-summary.txt'

$summary = @(
    'muldex interactive shell trial bundle'
    ''
    "binary: $binaryPath"
    ''
    'operator docs:'
    "- $(Join-Path $docRoot 'interactive-shell-guide.md')"
    "- $(Join-Path $docRoot 'interactive-shell-validation.md')"
    "- $(Join-Path $docRoot 'interactive-shell-release-checklist.md')"
    "- $(Join-Path $docRoot 'windows-terminal-performance.md')"
    "- $(Join-Path $docRoot 'codex-tui-compatibility-matrix.md')"
    ''
    'recommended launch:'
    'target\debug\muldex.exe'
    ''
    'result: validation passed and trial assets are ready for controlled operator trial'
)

$summary | Set-Content -LiteralPath $summaryPath -Encoding UTF8

"trial.binary: $binaryPath"
"trial.summary: $summaryPath"
"trial.result: ready-for-controlled-operator-trial"
