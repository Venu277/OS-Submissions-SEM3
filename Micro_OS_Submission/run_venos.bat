@echo off
title VenOS UEFI QEMU Runner
echo ==================================================
echo    Starting VenOS v0.1 in QEMU (UEFI Mode)
echo ==================================================
cd /d "C:\Users\Venugopal\OneDrive\Desktop\Micro_OS_Submission"

"C:\Program Files\qemu\qemu-system-x86_64.exe" ^
  -drive if=pflash,format=raw,unit=0,readonly=on,file="C:\Program Files\qemu\share\edk2-x86_64-code.fd" ^
  -drive format=raw,file="dist\venos-uefi.img" ^
  -drive format=raw,file="disk.img",if=ide,index=1,media=disk ^
  -device e1000,netdev=net0 ^
  -netdev user,id=net0 ^
  -serial stdio ^
  -m 512M

pause
