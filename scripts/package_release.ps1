# VenOS Distribution Packaging Automation Script
param(
    [string]$OutDir = "dist"
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "   VenOS Distribution Packaging" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan

# 1. Build release UEFI image
Write-Host "[1/4] Building release UEFI disk image..." -ForegroundColor Yellow
powershell -ExecutionPolicy Bypass -File scripts\build_uefi.ps1 -Profile "release" -OutImage "target\venos-uefi-release.img"

# 2. Ensure output directory exists
if (-not (Test-Path $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
}

# 3. Copy and generate checksums
Write-Host "[2/4] Generating release artifacts and SHA-256 checksums..." -ForegroundColor Yellow
$sha256 = (Get-FileHash -Path "target\venos-uefi-release.img" -Algorithm SHA256).Hash
$imgSize = (Get-Item "target\venos-uefi-release.img").Length

Copy-Item -Path "target\venos-uefi-release.img" -Destination "$OutDir\venos-uefi.img" -Force
Copy-Item -Path "target\venos-uefi-release.img" -Destination "$OutDir\venos-uefi.iso" -Force
Copy-Item -Path "target\venos-uefi-release.img" -Destination "target\venos-uefi.img" -Force

Set-Content -Path "$OutDir\SHA256SUMS" -Value "$sha256  venos-uefi.img`n$sha256  venos-uefi.iso"
Write-Host "  venos-uefi.img SHA256: $sha256" -ForegroundColor Gray
Write-Host "  venos-uefi.img Size:   $imgSize bytes ($([math]::Round($imgSize / 1MB, 2)) MB)" -ForegroundColor Gray

# 4. Generate README for distribution package
Write-Host "[3/4] Generating release README..." -ForegroundColor Yellow
$readmeContent = @"
# VenOS — Bare-Metal UEFI Release Package

## Files in this package:
- `venos-uefi.img`: Bootable 64-bit UEFI disk image (FAT32 GPT, \EFI\BOOT\BOOTX64.EFI)
- `SHA256SUMS`: Cryptographic checksum verification file

## How to Flash to USB:
### Windows (Rufus):
1. Download and open Rufus (portable).
2. Select your USB drive under 'Device'.
3. Click 'SELECT' and choose `venos-uefi.img`.
4. Click 'START'. When prompted, choose **Write in DD Image mode**.
5. Once complete, safely eject the USB drive.

### Linux / macOS:
```bash
sudo dd if=venos-uefi.img of=/dev/sdX bs=4M status=progress conv=fsync
```
*(Replace `/dev/sdX` with your target USB drive identifier).*

## How to Boot on Real Hardware:
1. Insert the USB drive into your laptop/PC.
2. Enter BIOS/UEFI Settings (F10 on HP, F2/F12 on Dell/Lenovo).
3. **Disable Secure Boot** (required for unsigned hobby kernel).
4. Save changes, restart, and enter Boot Menu (F9 on HP).
5. Select the USB Drive (UEFI OS).
6. VenOS will boot directly into the high-resolution graphical GOP text console!
"@

Set-Content -Path "$OutDir\README.txt" -Value $readmeContent

# 5. Create Distribution Zip Archive
Write-Host "[4/4] Creating distribution ZIP archive..." -ForegroundColor Yellow
$zipPath = "$OutDir\venos-uefi-v0.1.0-x86_64.zip"
$tempZipDir = Join-Path $env:TEMP "venos_pkg_temp"
if (Test-Path $tempZipDir) { Remove-Item $tempZipDir -Recurse -Force }
New-Item -ItemType Directory -Path $tempZipDir -Force | Out-Null
Copy-Item "target\venos-uefi-release.img" (Join-Path $tempZipDir "venos-uefi.img")
Copy-Item "$OutDir\SHA256SUMS" (Join-Path $tempZipDir "SHA256SUMS")
Copy-Item "$OutDir\README.txt" (Join-Path $tempZipDir "README.txt")

if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
Compress-Archive -Path "$tempZipDir\*" -DestinationPath $zipPath
Remove-Item $tempZipDir -Recurse -Force

$zipSize = (Get-Item $zipPath).Length
Write-Host "==================================================" -ForegroundColor Green
Write-Host "   Distribution Package Ready!" -ForegroundColor Green
Write-Host "   ZIP:  $zipPath" -ForegroundColor Green
Write-Host "   Size: $zipSize bytes ($([math]::Round($zipSize / 1MB, 2)) MB)" -ForegroundColor Green
Write-Host "==================================================" -ForegroundColor Green
