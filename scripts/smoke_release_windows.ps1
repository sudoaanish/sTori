param(
    [string]$ReleaseDirectory = "",
    [string]$ValidationRoot = "",
    [int]$StartupTimeoutSeconds = 30,
    [int]$StarterTimeoutSeconds = 120
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path (Join-Path $scriptDir "..")
if ([string]::IsNullOrWhiteSpace($ReleaseDirectory)) {
    $package = Get-Content -LiteralPath (Join-Path $repoRoot "package.json") -Raw | ConvertFrom-Json
    $ReleaseDirectory = Join-Path $repoRoot (Join-Path ".release" (Join-Path "v$($package.version)" "windows-x64"))
}

$app = Join-Path $ReleaseDirectory "stori.exe"
if (-not (Test-Path -LiteralPath $app)) { throw "Release executable not found: $app" }
if (Get-NetTCPConnection -LocalPort 1822 -State Listen -ErrorAction SilentlyContinue) {
    throw "Port 1822 is already in use. Stop the development server before running the isolated release smoke test."
}

$runId = [DateTime]::UtcNow.ToString("yyyyMMdd-HHmmss")
if ([string]::IsNullOrWhiteSpace($ValidationRoot)) {
    $ValidationRoot = Join-Path (Split-Path -Parent $ReleaseDirectory) "validation"
}
$sandbox = Join-Path $ValidationRoot $runId
$data = Join-Path $sandbox "app-data"
$books = Join-Path $sandbox "sTori Books"
New-Item -ItemType Directory -Force -Path $data, $books | Out-Null

$env:STORI_DATA_DIR = $data
$env:STORI_MANAGED_LIBRARY_DIR = $books
$process = Start-Process -FilePath $app -WorkingDirectory $ReleaseDirectory -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds($StartupTimeoutSeconds)
    do {
        Start-Sleep -Milliseconds 300
        if ($process.HasExited) { throw "Release app exited before its server became ready (exit code $($process.ExitCode))." }
        try { $health = Invoke-RestMethod "http://127.0.0.1:1822/api/health" -TimeoutSec 2 } catch { $health = $null }
    } until ($health.status -eq "ok" -or [DateTime]::UtcNow -gt $deadline)
    if ($null -eq $health -or $health.status -ne "ok") { throw "Release server did not become healthy within $StartupTimeoutSeconds seconds." }

    $diagnostics = Invoke-RestMethod "http://127.0.0.1:1822/api/admin/diagnostics"
    $libraries = Invoke-RestMethod "http://127.0.0.1:1822/api/admin/libraries"
    $manifestResponse = Invoke-WebRequest "http://127.0.0.1:1822/manifest.webmanifest" -UseBasicParsing
    if ($diagnostics.database_status -ne "ok") { throw "Release database integrity check failed." }
    if ([int]$diagnostics.schema_version -ne 3) { throw "Unexpected release schema version: $($diagnostics.schema_version)" }
    if (-not (Test-Path -LiteralPath (Join-Path $data "stori.db"))) { throw "First-run database was not created in the isolated data directory." }
    if ($manifestResponse.StatusCode -ne 200) { throw "Packaged PWA manifest was not served." }

    $starterDeadline = [DateTime]::UtcNow.AddSeconds($StarterTimeoutSeconds)
    do {
        Start-Sleep -Milliseconds 500
        $jobs = Invoke-RestMethod "http://127.0.0.1:1822/api/admin/downloads"
        $failed = @($jobs | Where-Object { $_.status -eq "failed" })
        if ($failed.Count -gt 0) { throw "A first-run starter download failed: $($failed[0].error)" }
        $completed = @($jobs | Where-Object { $_.status -eq "completed" })
    } until (($jobs.Count -eq 2 -and $completed.Count -eq 2) -or [DateTime]::UtcNow -gt $starterDeadline)
    if ($jobs.Count -ne 2 -or $completed.Count -ne 2) { throw "Starter shelf did not complete within $StarterTimeoutSeconds seconds." }
    $booksIndexed = Invoke-RestMethod "http://127.0.0.1:1822/api/books"
    if ($booksIndexed.Count -lt 2) { throw "Starter EPUBs completed but were not indexed." }

    [ordered]@{
        result = "passed"
        tested_utc = [DateTime]::UtcNow.ToString("o")
        app = $app
        version = $health.version
        schema = $diagnostics.schema_version
        database = $diagnostics.database_status
        libraries = $libraries.Count
        starter_jobs = $jobs.Count
        indexed_books = $booksIndexed.Count
        pwa_manifest = "ok"
        isolated_data = $data
        isolated_books = $books
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $sandbox "smoke-result.json") -Encoding UTF8
    Write-Host "Isolated release smoke test passed: $sandbox" -ForegroundColor Green
} finally {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id }
    Remove-Item Env:STORI_DATA_DIR -ErrorAction SilentlyContinue
    Remove-Item Env:STORI_MANAGED_LIBRARY_DIR -ErrorAction SilentlyContinue
}
