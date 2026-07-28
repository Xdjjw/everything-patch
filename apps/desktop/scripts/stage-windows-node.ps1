[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$version = '22.23.1'
$expectedSha256 = '7df0bc9375723f4a86b3aa1b7cc73342423d9677a8df4538aca31a049e309c29'
$expectedNodeSha256 = 'F8D162C0641DCEE512132F3BCF8A68169C7ECB852EFD8E1A46C9FEC5A0F469ED'
$expectedLicenseSha256 = '8CC9BB466B19FC7E7CC99D03E9DF1132021FDA8B01EEA2624C58BB372DBEF576'
$desktopRoot = Split-Path -Parent $PSScriptRoot
$targetRoot = Join-Path $desktopRoot 'src-tauri\runtime\windows\node'
$nodePath = Join-Path $targetRoot 'node.exe'
$licensePath = Join-Path $targetRoot 'LICENSE'
$expectedVersion = "v$version"

function Get-Sha256 {
    param([Parameter(Mandatory = $true)][string]$Path)

    $stream = [IO.File]::OpenRead($Path)
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha256.ComputeHash($stream))).Replace('-', '')
    }
    finally {
        $sha256.Dispose()
        $stream.Dispose()
    }
}

function Test-StagedRuntime {
    if (-not (Test-Path -LiteralPath $nodePath -PathType Leaf)) {
        return $false
    }
    if (-not (Test-Path -LiteralPath $licensePath -PathType Leaf)) {
        return $false
    }
    try {
        if ((Get-Sha256 -Path $nodePath) -cne $expectedNodeSha256) {
            return $false
        }
        if ((Get-Sha256 -Path $licensePath) -cne $expectedLicenseSha256) {
            return $false
        }
        return ((& $nodePath --version).Trim() -ceq $expectedVersion)
    }
    catch {
        return $false
    }
}

if (Test-StagedRuntime) {
    Write-Host "Windows Node.js runtime already staged: $expectedVersion"
    exit 0
}

$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$workRoot = Join-Path $tempRoot "everything-patch-node-$version-$PID-$([Guid]::NewGuid().ToString('N'))"
$resolvedWorkRoot = [IO.Path]::GetFullPath($workRoot)
if (-not $resolvedWorkRoot.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to use a temporary directory outside the system temp root: $resolvedWorkRoot"
}

$archive = Join-Path $workRoot "node-v$version-win-x64.zip"
$extractRoot = Join-Path $workRoot 'extract'
$sourceRoot = Join-Path $extractRoot "node-v$version-win-x64"

try {
    New-Item -ItemType Directory -Path $workRoot -Force | Out-Null
    Invoke-WebRequest `
        -Uri "https://nodejs.org/dist/v$version/node-v$version-win-x64.zip" `
        -OutFile $archive

    $actualSha256 = (Get-Sha256 -Path $archive).ToLowerInvariant()
    if ($actualSha256 -cne $expectedSha256) {
        throw "Node.js archive checksum mismatch: $actualSha256"
    }

    Expand-Archive -LiteralPath $archive -DestinationPath $extractRoot
    New-Item -ItemType Directory -Path $targetRoot -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $sourceRoot 'node.exe') -Destination $nodePath -Force
    Copy-Item -LiteralPath (Join-Path $sourceRoot 'LICENSE') -Destination $licensePath -Force

    if (-not (Test-StagedRuntime)) {
        throw 'Pinned Windows Node.js runtime failed post-stage validation.'
    }
    Write-Host "Staged Windows Node.js runtime: $expectedVersion"
}
finally {
    if (
        (Test-Path -LiteralPath $resolvedWorkRoot) -and
        $resolvedWorkRoot.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)
    ) {
        Remove-Item -LiteralPath $resolvedWorkRoot -Recurse -Force
    }
}
