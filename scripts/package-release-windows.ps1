$ErrorActionPreference = 'Stop'

param(
    [Parameter(Mandatory = $true)]
    [string]$Target,
    [Parameter(Mandatory = $true)]
    [string]$ArtifactName
)

$repoRoot = Split-Path -Parent $PSScriptRoot
$binaryPath = Join-Path $repoRoot "target\$Target\release\muldex.exe"
if (-not (Test-Path -LiteralPath $binaryPath)) {
    throw "binary not found: $binaryPath"
}

$packageRoot = Join-Path $repoRoot "target\release-package\$ArtifactName"
$packageZip = "$packageRoot.zip"

if (Test-Path -LiteralPath $packageRoot) {
    Remove-Item -Recurse -Force -LiteralPath $packageRoot
}
if (Test-Path -LiteralPath $packageZip) {
    Remove-Item -Force -LiteralPath $packageZip
}

New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null
Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $packageRoot 'muldex.exe') -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'scripts\install-muldex-windows.ps1') -Destination (Join-Path $packageRoot 'install.ps1') -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'scripts\uninstall-muldex-windows.ps1') -Destination (Join-Path $packageRoot 'uninstall.ps1') -Force

$readmePath = Join-Path $packageRoot 'README.txt'
@(
    "muldex release artifact: $ArtifactName"
    ''
    'docs:'
    '- docs/interactive-shell-guide.md'
    '- docs/interactive-shell-validation.md'
    '- docs/interactive-shell-release-checklist.md'
    '- docs/installing-muldex-cli.md'
) | Set-Content -LiteralPath $readmePath -Encoding UTF8

Compress-Archive -Path "$packageRoot\*" -DestinationPath $packageZip

"package.result: ok"
"package.path: $packageZip"
