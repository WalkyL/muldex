param(
    [string]$InstallDir = "$env:USERPROFILE\.muldex\bin"
)

$ErrorActionPreference = 'Stop'

$targetBinary = Join-Path $InstallDir 'muldex.exe'
if (Test-Path -LiteralPath $targetBinary) {
    Remove-Item -LiteralPath $targetBinary -Force
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath) {
    $newPath = ($userPath.Split(';') | Where-Object { $_ -and $_ -ne $InstallDir }) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
}

"uninstall.result: ok"
"uninstall.path_removed: $InstallDir"
"uninstall.note: directory cleanup is left to the operator if other files remain"
