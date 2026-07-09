$ErrorActionPreference = 'Stop'

param(
    [Parameter(Mandatory = $true)]
    [string]$ArtifactPath,
    [Parameter(Mandatory = $true)]
    [string]$PlatformKind
)

if (-not (Test-Path -LiteralPath $ArtifactPath)) {
    throw "artifact not found: $ArtifactPath"
}

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("muldex-artifact-check-" + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null

try {
    if ($ArtifactPath.EndsWith('.zip')) {
        Expand-Archive -LiteralPath $ArtifactPath -DestinationPath $tempRoot -Force
    }
    elseif ($ArtifactPath.EndsWith('.tar.gz')) {
        tar -xzf $ArtifactPath -C $tempRoot
    }
    else {
        throw "unsupported artifact extension: $ArtifactPath"
    }

    $entries = Get-ChildItem -LiteralPath $tempRoot -Recurse -File
    $names = $entries | ForEach-Object { $_.Name }

    if ($PlatformKind -eq 'windows') {
        if ($names -notcontains 'muldex.exe') { throw 'missing muldex.exe' }
        if ($names -notcontains 'install.ps1') { throw 'missing install.ps1' }
        if ($names -notcontains 'uninstall.ps1') { throw 'missing uninstall.ps1' }
    }
    else {
        if ($names -notcontains 'muldex') { throw 'missing muldex binary' }
        if ($names -notcontains 'install.sh') { throw 'missing install.sh' }
        if ($names -notcontains 'uninstall.sh') { throw 'missing uninstall.sh' }
    }

    if ($names -notcontains 'README.txt') { throw 'missing README.txt' }

    "artifact.verify: ok"
    "artifact.path: $ArtifactPath"
}
finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -Recurse -Force -LiteralPath $tempRoot
    }
}
