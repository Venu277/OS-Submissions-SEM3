# VenOS Automated UEFI Build Script
param(
    [string]$Profile = "debug",
    [string]$OutImage = "target\venos-uefi.img"
)

$ErrorActionPreference = "Stop"

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "   VenOS UEFI Disk Image Builder" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan

# 1. Ensure host builder binary exists
$builderExe = "builder\target\x86_64-pc-windows-msvc\release\builder.exe"
if (-not (Test-Path $builderExe)) {
    Write-Host "[1/3] Building host builder tool..." -ForegroundColor Yellow
    cargo +nightly build --release --manifest-path builder\Cargo.toml --target x86_64-pc-windows-msvc
} else {
    Write-Host "[1/3] Host builder tool is ready." -ForegroundColor Green
}

# 2. Build kernel ELF
Write-Host "[2/3] Compiling VenOS kernel ($Profile profile)..." -ForegroundColor Yellow
if ($Profile -eq "release") {
    cargo build --release
    $kernelElf = "target\x86_64-micro_os\release\micro_os"
} else {
    cargo build
    $kernelElf = "target\x86_64-micro_os\debug\micro_os"
}

if (-not (Test-Path $kernelElf)) {
    Write-Error "Kernel binary not found at '$kernelElf'"
    exit 1
}

$elfItem = Get-Item $kernelElf
Write-Host "  Kernel ELF size: $($elfItem.Length) bytes ($([math]::Round($elfItem.Length / 1MB, 2)) MB)" -ForegroundColor Gray

# 3. Generate UEFI disk image
Write-Host "[3/3] Packaging UEFI GPT disk image..." -ForegroundColor Yellow
& $builderExe $kernelElf $OutImage

if (-not (Test-Path $OutImage)) {
    Write-Error "Failed to generate UEFI disk image '$OutImage'"
    exit 1
}

$imgItem = Get-Item $OutImage
Write-Host "==================================================" -ForegroundColor Green
Write-Host "   SUCCESS: UEFI Disk Image Created!" -ForegroundColor Green
Write-Host "   Path: $OutImage" -ForegroundColor Green
Write-Host "   Size: $($imgItem.Length) bytes ($([math]::Round($imgItem.Length / 1MB, 2)) MB)" -ForegroundColor Green
Write-Host "==================================================" -ForegroundColor Green
