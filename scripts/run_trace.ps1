# scripts/run_trace.ps1
# Automates execution of MicroOS in QEMU with serial output piped to a trace log file.
# Specification: CA-6 Section 9 (Layer 2) & Section 23 (Trace Analysis)

param (
    [string]$TraceName = "step_a2_diagnostics",
    [int]$DurationSeconds = 4,
    [switch]$CoreMode
)

$ErrorActionPreference = "Stop"

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "   MicroOS Automated Trace Capture Runner" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan

# 1. Build kernel bootimage if needed
if ($CoreMode) {
    Write-Host "[BUILD] Compiling kernel in ca6-core mode..." -ForegroundColor Yellow
    cargo bootimage --features ca6-core
} else {
    Write-Host "[BUILD] Compiling kernel standard build..." -ForegroundColor Yellow
    cargo bootimage
}

$binPath = "target\x86_64-micro_os\debug\bootimage-micro_os.bin"
if (-not (Test-Path $binPath)) {
    Write-Error "Bootimage binary not found at $binPath!"
}

# 2. Ensure traces directory exists
if (-not (Test-Path "traces")) {
    New-Item -ItemType Directory -Path "traces" | Out-Null
}

$traceFile = "traces\$TraceName.log"
if (Test-Path $traceFile) {
    Remove-Item -Force $traceFile
}

Write-Host "[QEMU] Starting QEMU execution with serial output to $traceFile..." -ForegroundColor Yellow

$qemuArgs = @(
    "-drive", "format=raw,file=$binPath",
    "-serial", "stdio",
    "-display", "none",
    "-drive", "format=raw,file=disk.img,if=ide,index=1,media=disk"
)

$p = Start-Process -FilePath "qemu-system-x86_64" -ArgumentList $qemuArgs -RedirectStandardOutput $traceFile -PassThru

Write-Host "[TRACE] Collecting trace data for $DurationSeconds seconds..." -ForegroundColor Green
Start-Sleep -Seconds $DurationSeconds

if (-not $p.HasExited) {
    Stop-Process -Id $p.Id -Force
}

if (Test-Path $traceFile) {
    $item = Get-Item $traceFile
    Write-Host "[SUCCESS] Trace captured: $traceFile ($($item.Length) bytes)" -ForegroundColor Cyan
    Get-Content $traceFile | Select-Object -First 20
} else {
    Write-Error "Failed to generate trace file $traceFile"
}
