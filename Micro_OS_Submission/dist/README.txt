# VenOS — Bare-Metal UEFI Release Package

## Files in this package:
- enos-uefi.img: Bootable 64-bit UEFI disk image (FAT32 GPT, \EFI\BOOT\BOOTX64.EFI)
- SHA256SUMS: Cryptographic checksum verification file

## How to Flash to USB:
### Windows (Rufus):
1. Download and open Rufus (portable).
2. Select your USB drive under 'Device'.
3. Click 'SELECT' and choose enos-uefi.img.
4. Click 'START'. When prompted, choose **Write in DD Image mode**.
5. Once complete, safely eject the USB drive.

### Linux / macOS:
`ash
sudo dd if=venos-uefi.img of=/dev/sdX bs=4M status=progress conv=fsync
`
*(Replace /dev/sdX with your target USB drive identifier).*

## How to Boot on Real Hardware:
1. Insert the USB drive into your laptop/PC.
2. Enter BIOS/UEFI Settings (F10 on HP, F2/F12 on Dell/Lenovo).
3. **Disable Secure Boot** (required for unsigned hobby kernel).
4. Save changes, restart, and enter Boot Menu (F9 on HP).
5. Select the USB Drive (UEFI OS).
6. VenOS will boot directly into the high-resolution graphical GOP text console!
