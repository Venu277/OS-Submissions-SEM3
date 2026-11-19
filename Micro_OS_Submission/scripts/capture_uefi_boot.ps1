param(
    [string]$OutLog = "traces\step_v2_hw_discovery.log",
    [int]$TimeoutSec = 12
)

$ovmfCode = "C:\Program Files\qemu\share\edk2-x86_64-code.fd"
if (-not (Test-Path $ovmfCode)) {
    Write-Error "OVMF UEFI firmware not found at '$ovmfCode'"
    exit 1
}

$img = "target\venos-uefi.img"
if (-not (Test-Path $img)) {
    Write-Error "Disk image not found at '$img'"
    exit 1
}

Write-Host "Running VenOS in QEMU (capturing serial to $OutLog)..." -ForegroundColor Cyan

# Remove old trace if present
if (Test-Path $OutLog) {
    Remove-Item $OutLog -Force
}

$absLog = (Join-Path (Get-Location) $OutLog)

$p = Start-Process -FilePath "cmd.exe" -ArgumentList @(
    "/c",
    "qemu-system-x86_64 -drive if=pflash,format=raw,unit=0,readonly=on,file=`"$ovmfCode`" -drive format=raw,file=`"$img`" -serial stdio -display none -no-reboot > `"$absLog`" 2>&1"
) -PassThru

Start-Sleep -Seconds $TimeoutSec
Stop-Process -Name "qemu-system-x86_64" -Force -ErrorAction SilentlyContinue
if (-not $p.HasExited) {
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
}

Write-Host "Done. Trace saved to $OutLog" -ForegroundColor Green
Get-Content $OutLog
