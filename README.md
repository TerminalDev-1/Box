# Box! Workstation

A **100% pure native Rust** desktop virtual machine workstation for Windows, powered by **QEMU + Windows Hypervisor Platform (WHPX)**, **QMP**, and a **direct native TCP RFB/VNC GPU framebuffer renderer** (`eframe` / `egui` 0.37 on `wgpu` DX12).

Zero React. Zero TypeScript. Zero Node.js. Zero Electron. Zero WebView2.

## Features

- **Native Frameless DX12 Light-Mode UI:** Custom 34px titlebar with drag/maximize/8-edge resize, hand-painted vector icons, native Windows Segoe UI & Consolas fonts, resizable Library sidebar, and multi-tab workspace.
- **Real QEMU + WHPX Virtualization:** Hardware APIC (`-accel whpx`), Hyper-V enlightenments (`hv_relaxed`, `hv_spinlocks`, `hv_vapic`, `hv_time`), single-socket multi-core topology, Q35 PCIe / i440FX chipsets, SeaBIOS + OVMF UEFI (`efivars.qcow2`), and Windows 11 ISO support.
- **VMware-Style 9-Device Hardware Settings:** Configure Memory, Processors & Acceleration, Hard Disk (QCOW2 + VirtIO/SATA/IDE), CD/DVD ISO, Network Adapter (`e1000` / VirtIO / `rtl8139`), USB Controller (`xHCI 3.0` / `EHCI 2.0`), Sound Card (Intel HDA / AC'97 via DirectSound), Display/GPU with native EDID resolution (`1920×1080`, `2560×1440`, etc.), and Firmware/Boot Order.
- **Embedded 60fps GPU VNC Console:** Direct pure-Rust TCP RFB 3.8 client with `TextureHandle::set_partial` dirty-rect updates, 0% idle CPU, Fullscreen mode (`Ctrl+Alt+Enter` with floating hover bar), and one-click `Shift+F10` & `Ctrl+Alt+Del` QMP keys.
- **Live WHPX Disk Snapshots:** QMP `blockdev-snapshot-internal-sync` / `blockdev-snapshot-delete-internal-sync` + offline `qemu-img snapshot` management.

## Requirements

- Windows 10/11 with **Windows Hypervisor Platform (WHPX)** enabled (falls back to TCG if unavailable)
- [QEMU for Windows](https://www.qemu.org/download/#windows) (`qemu-system-x86_64.exe` & `qemu-img.exe`)
- Rust toolchain (`x86_64-pc-windows-msvc`, Rust 1.95+)

## Build & Run

```powershell
cargo build --release
.\target\release\box-vm.exe
```
