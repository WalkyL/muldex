param(
    [string]$BinaryPath,
    [string]$InstallDir,
    [string]$ConfigPath,
    [switch]$UseLlmRouter,
    [switch]$SkipLlmRouterPrompt
)

$ErrorActionPreference = 'Stop'

if (-not $BinaryPath) {
    $BinaryPath = '.\target\debug\muldex.exe'
}

if (-not $InstallDir) {
    $InstallDir = "$env:USERPROFILE\.muldex\bin"
}

if (-not $ConfigPath) {
    $ConfigPath = "$env:USERPROFILE\.muldex\config.json"
}

function Write-LlmRouterConfig {
    param(
        [string]$Path,
        [string]$Host,
        [int]$Port,
        [string]$ApiKey,
        [string]$DefaultModel
    )

    $parent = Split-Path -Parent $Path
    if (-not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }

    if (Test-Path -LiteralPath $Path) {
        $config = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json -AsHashtable
    } else {
        $config = [ordered]@{
            schema_version = 'muldex-config-v1'
            default_provider = 'llm-router'
            providers = [ordered]@{}
        }
    }

    if (-not $config.Contains('providers')) {
        $config['providers'] = [ordered]@{}
    }

    $config['default_provider'] = 'llm-router'
    $config['providers']['llm-router'] = [ordered]@{
        kind = 'openai-compatible'
        host = $Host
        port = $Port
        api_key = $ApiKey
        default_model = $(if ($DefaultModel) { $DefaultModel } else { $null })
    }

    $config | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $Path -Encoding UTF8
}

function Test-LlmRouterConnectivity {
    param(
        [string]$Host,
        [int]$Port
    )

    $client = New-Object System.Net.Sockets.TcpClient
    try {
        $async = $client.BeginConnect($Host, $Port, $null, $null)
        if (-not $async.AsyncWaitHandle.WaitOne(2000, $false)) {
            return 'timeout'
        }
        $client.EndConnect($async)
        return 'reachable'
    }
    catch {
        return "unreachable: $($_.Exception.Message)"
    }
    finally {
        $client.Close()
    }
}

if (-not (Test-Path -LiteralPath $BinaryPath)) {
    throw "binary not found: $BinaryPath"
}

if (-not (Test-Path -LiteralPath $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$targetBinary = Join-Path $InstallDir 'muldex.exe'
Copy-Item -LiteralPath $BinaryPath -Destination $targetBinary -Force

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$pathEntries = @()
if ($userPath) {
    $pathEntries = $userPath.Split(';') | Where-Object { $_ -and $_.Trim() -ne '' }
}

if ($pathEntries -notcontains $InstallDir) {
    $newPath = @($pathEntries + $InstallDir) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
}

$enableLlmRouter = $UseLlmRouter.IsPresent

if (-not $SkipLlmRouterPrompt.IsPresent -and -not $UseLlmRouter.IsPresent) {
    ''
    'muldex uses an OpenAI-compatible request shape.'
    'Many model providers are not fully compatible with that shape in practice.'
    'llm-router is the recommended compatibility layer for request and response normalization.'
    $choice = Read-Host 'Configure llm-router as the default provider now? [Y/n]'
    if ([string]::IsNullOrWhiteSpace($choice) -or $choice -match '^(y|yes)$') {
        $enableLlmRouter = $true
    }
}

if ($enableLlmRouter) {
    $routerHost = Read-Host 'llm-router host/IP [127.0.0.1]'
    if ([string]::IsNullOrWhiteSpace($routerHost)) {
        $routerHost = '127.0.0.1'
    }

    $routerPort = Read-Host 'llm-router port [3000]'
    if ([string]::IsNullOrWhiteSpace($routerPort)) {
        $routerPort = '3000'
    }

    $routerApiKey = Read-Host 'llm-router API key (leave blank to set later)'
    $routerDefaultModel = Read-Host 'default model (optional, leave blank to set later)'

    Write-LlmRouterConfig -Path $ConfigPath -Host $routerHost -Port ([int]$routerPort) -ApiKey $routerApiKey -DefaultModel $routerDefaultModel
    $routerConnectivity = Test-LlmRouterConnectivity -Host $routerHost -Port ([int]$routerPort)
}

"install.result: ok"
"install.binary: $targetBinary"
"install.path_added: $InstallDir"
if ($enableLlmRouter) {
    "install.llm_router: configured"
    "install.config_path: $ConfigPath"
    "install.llm_router.connectivity: $routerConnectivity"
    "install.next_step: verify with /config llm test or /provider test inside the shell"
} else {
    "install.llm_router: skipped"
    "install.next_step: manually configure a provider before normal shell use"
}
"install.note: restart terminal to pick up updated PATH"
