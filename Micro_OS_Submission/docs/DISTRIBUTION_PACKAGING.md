# VenOS — Distribution & Packaging Guide

## Overview
This document specifies the distribution packaging format, image layout, flashing procedures, and hardware execution requirements for **VenOS v0.1 (x86_64)**.

---

## 1. UEFI Disk Image Specification

- **File Name:** `venos-uefi.img` (or `venos-uefi-release.img`)
- **Format:** Raw raw disk image with a valid **GUID Partition Table (GPT)**.
- **Partition Layout:**
  - Partition 1: **EFI System Partition (ESP)**
    - Filesystem: FAT32 (MS-DOS 3.31 compatible)
    - Type GUID: `c12a7328-f81f-11d2-ba4b-00a0c93ec93b`
    - Partition Flags: Bootable / Active
    - Standard UEFI Entry: `\EFI\BOOT\BOOTX64.EFI`
- **Bootloader Implementation:** Built using `bootloader_api 0.11.17` and `bootloader::UefiBoot`. It transitions the x86_64 CPU directly into 64-bit Long Mode, establishes a 4-level page table hierarchy, maps physical memory dynamically, queries the Graphics Output Protocol (GOP) framebuffer, and jumps into the kernel entry point at `_start`.

---

## 2. Automated Build Pipeline

The UEFI image is constructed deterministically via the host builder crate:
```powershell
# Build debug or release UEFI disk image
powershell -ExecutionPolicy Bypass -File scripts\build_uefi.ps1 -Profile "release"
```
Or package the full distribution bundle with checksums and zip archive:
```powershell
powershell -ExecutionPolicy Bypass -File scripts\package_release.ps1
```
Output artifacts generated in `dist/`:
- `dist/venos-uefi.img` (12.0 MB FAT32 GPT image)
- `dist/SHA256SUMS` (SHA-256 integrity hash)
- `dist/README.txt` (End-user flashing instructions)
- `dist/venos-uefi-v0.1.0-x86_64.zip` (Compressed distribution archive)

---

## 3. Flashing to Physical USB Drive

### Method A: Windows via Rufus (Verified on Real Hardware)
1. Download and run **Rufus** (portable version 4.x).
2. Under **Device**, select your USB flash drive (minimum 512 MB).
3. Under **Boot selection**, click **SELECT** and choose `venos-uefi.img`.
4. Partition scheme: **MBR** or **GPT** (Rufus automatically sets Target system: BIOS or UEFI-CSM).
5. Click **START**.
6. When prompted with the dialog `"Write in ISO Image mode"` vs `"Write in DD Image mode"`, choose:
   > **Write in DD Image mode (Recommended)**
7. Rufus will raw-copy the GPT partition table and FAT32 EFI boot sector directly to the drive blocks.

### Method B: Linux / macOS via `dd`
```bash
# Verify USB drive identifier (e.g. /dev/sdb or /dev/sdc)
lsblk

# Flash directly to raw block device (NOT a partition like /dev/sdb1)
sudo dd if=venos-uefi.img of=/dev/sdX bs=4M status=progress conv=fsync
```

---

## 4. Booting on Bare-Metal Hardware

### Hardware Compatibility Checklist
- **CPU:** Any modern x86_64 processor (Intel Core 2 Duo and newer, AMD Athlon 64 and newer; verified on 13th Gen Intel Core i5-1335U).
- **Firmware:** UEFI Class 2 (with CSM disabled) or UEFI Class 3 (Pure UEFI, no CSM).
- **RAM:** Minimum 256 MiB (adaptively scales up to 4 MiB kernel heap).
- **Display:** Any VESA/GOP-compliant display (verified on 1920x1200 16:10 Intel Iris Xe).
- **Keyboard:** Standard PS/2 keyboard or EC PS/2 emulation (`ACPI\HPQ8001`).

### BIOS / UEFI Setup Instructions
1. Insert the prepared USB flash drive.
2. Turn on the PC and press the BIOS setup hotkey:
   - **HP:** Press `Esc` repeatedly, then press `F10`.
   - **Dell:** Press `F2`.
   - **Lenovo:** Press `F1` or `Enter`.
   - **ASUS / Acer:** Press `F2` or `Del`.
3. Navigate to **Boot Options** / **Security**:
   - **Secure Boot:** **DISABLE** (essential: the custom kernel binary is self-contained and unsigned by Microsoft 3rd Party UEFI CA).
   - **Fast Boot:** Can be disabled to ensure USB controller initialization.
4. Save settings and restart.
5. Press the Boot Menu hotkey (`F9` on HP, `F12` on Dell/Lenovo).
6. Select your USB drive from the UEFI boot options list.
7. VenOS boots immediately into the graphical GOP text console!

---

## 5. Cloud Hosting & Submission Links
The packaged disk image can be uploaded to:
- **GitHub Releases:** Tag `v0.1.0` with `venos-uefi-v0.1.0-x86_64.zip`.
- **Hugging Face Hub:** Hosted under datasets or models as a public binary artifact.
