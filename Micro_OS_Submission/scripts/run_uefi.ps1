# VenOS UEFI QEMU Runner Script
param(
    [string]$Image = "dist\venos-uefi.img",
    [string]$Display = "default",
    [switch]$NoReboot = $true
)

$ErrorActionPreference = "Stop"

$ovmfCode = "C:\Program Files\qemu\share\edk2-x86_64-code.fd"
if (-not (Test-Path $ovmfCode)) {
    Write-Error "OVMF UEFI firmware not found at '$ovmfCode'"
    exit 1
}

if (-not (Test-Path $Image)) {
    Write-Host "Image not found at '$Image', running build script..." -ForegroundColor Yellow
    & powershell -ExecutionPolicy Bypass -File "scripts\build_uefi.ps1"
}

Write-Host "Booting VenOS UEFI in QEMU (Firmware: OVMF, Display: $Display)..." -ForegroundColor Cyan

$qemuArgs = @(
    "-drive", "if=pflash,format=raw,unit=0,readonly=on,file=$ovmfCode",
    "-drive", "format=raw,file=$Image",
    "-serial", "stdio"
)

if ($Display -eq "none") {
    $qemuArgs += ("-display", "none")
}

if ($NoReboot) {
    $qemuArgs += "-no-reboot"
}

& qemu-system-x86_64 @qemuArgs
