param(
    [string]$ReleaseRoot = "",
    [switch]$SkipTests,
    [switch]$SkipBuild,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path (Join-Path $scriptDir "..")
$package = Get-Content -LiteralPath (Join-Path $repoRoot "package.json") -Raw | ConvertFrom-Json
$tauri = Get-Content -LiteralPath (Join-Path $repoRoot "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$cargo = Get-Content -LiteralPath (Join-Path $repoRoot "src-tauri\Cargo.toml") -Raw
$version = [string]$package.version

if ([string]::IsNullOrWhiteSpace($ReleaseRoot)) {
    $ReleaseRoot = Join-Path $repoRoot ".release"
}

if ([string]$tauri.version -ne $version -or $cargo -notmatch "(?m)^version = `"$([regex]::Escape($version))`"$") {
    throw "Version mismatch: package.json, tauri.conf.json, and Cargo.toml must all use $version."
}

$destination = Join-Path $ReleaseRoot (Join-Path "v$version" "windows-x64")
if ((Test-Path -LiteralPath $destination) -and -not $Force) {
    throw "Release destination already exists: $destination. Use -Force only when intentionally replacing this private build."
}
New-Item -ItemType Directory -Force -Path $destination | Out-Null

Push-Location $repoRoot
try {
    $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
    if (Test-Path -LiteralPath $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }
    if (-not $SkipTests) {
        & npm.cmd test -- --run
        if ($LASTEXITCODE -ne 0) { throw "Frontend tests failed." }
        & "$env:USERPROFILE\.cargo\bin\cargo.exe" test --manifest-path src-tauri/Cargo.toml
        if ($LASTEXITCODE -ne 0) { throw "Rust tests failed." }
    }

    if (-not $SkipBuild) {
        & npm.cmd run tauri build -- --bundles nsis
        if ($LASTEXITCODE -ne 0) { throw "Tauri NSIS build failed." }
    }

    $releaseBinary = Join-Path $repoRoot "src-tauri\target\release\stori.exe"
    $resourceDirectory = Join-Path $repoRoot "src-tauri\target\release\_up_"
    $installer = Join-Path $repoRoot "src-tauri\target\release\bundle\nsis\sTori_${version}_x64-setup.exe"
    foreach ($required in @($releaseBinary, $installer)) {
        if (-not (Test-Path -LiteralPath $required)) { throw "Expected release artifact is missing: $required" }
    }

    Copy-Item -LiteralPath $releaseBinary -Destination $destination -Force
    Copy-Item -LiteralPath $installer -Destination $destination -Force
    if (Test-Path -LiteralPath $resourceDirectory) {
        Copy-Item -LiteralPath $resourceDirectory -Destination $destination -Recurse -Force
    }
    Copy-Item -LiteralPath (Join-Path $repoRoot "THIRD_PARTY_FONTS.md") -Destination $destination -Force

    $manifestPath = Join-Path $destination "release-manifest.json"
    $files = Get-ChildItem -LiteralPath $destination -File -Recurse | Where-Object {
        $_.FullName -ne $manifestPath -and
        -not $_.FullName.StartsWith((Join-Path $destination "_smoke") + '\')
    } | ForEach-Object {
        [ordered]@{
            path = $_.FullName.Substring($destination.Length).TrimStart('\')
            size_bytes = $_.Length
            sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    $manifest = [ordered]@{
        product = "sTori"
        version = $version
        channel = "private-validation"
        built_utc = [DateTime]::UtcNow.ToString("o")
        source = $repoRoot.Path
        files = @($files)
    }
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifestPath -Encoding UTF8
    Write-Host "Private Windows release created: $destination" -ForegroundColor Green
} finally {
    Pop-Location
}
