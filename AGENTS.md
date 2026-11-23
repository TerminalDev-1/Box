# AGENTS.md — Box! Workstation (`BoxVM`)

> **Box!** is a **100% pure native Rust** desktop virtual machine workstation for Windows, powered by **QEMU + Windows Hypervisor Platform (WHPX)**, **QMP**, and a **direct native TCP RFB/VNC GPU framebuffer renderer** (`eframe` / `egui` 0.37 on `wgpu` DX12).
>
> **Hard Rule:** Zero React. Zero TypeScript. Zero Node.js. Zero Electron. Zero Tauri/WebView2. Zero VirtualBox. Pure native Rust only.

---

## 1. Repository & Architecture Map

- **[Cargo.toml](Cargo.toml)** — Pure Rust manifest (`eframe` 0.37 with `wgpu` DX12, `egui` 0.37, `rfd` 0.18 native Windows file dialogs, `serde`/`serde_json`, `uuid`, `chrono`, `dirs`, `sysinfo`).
- **[src/main.rs](src/main.rs)** — Entry point (`eframe::run_native`), frameless native window (`with_decorations(false)`), comfortable DPI-aware default size (`1040×660`, min `760×480`), generated isometric RGBA window icon.
- **[src/theme.rs](src/theme.rs)** — **Light Mode Only** Windows 11 / VMware Workstation theme (`Visuals::light()`, pure white `#FFFFFF` cards on `#F3F5F8` canvas, native `C:\Windows\Fonts\segoeui.ttf` + `consola.ttf` fonts, hand-painted vector icons via `egui::Painter` — zero Unicode tofu glyphs, zero raster PNGs).
- **[src/app.rs](src/app.rs)** — Complete workstation UI:
  - Custom 34px frameless titlebar with `File` / `View` / `VM` menus, drag-to-move, double-click maximize, 8-edge manual window resizing, and custom painted Minimize/Maximize/Close buttons.
  - 38px workstation toolbar with vector icon buttons + live Host CPU/RAM telemetry pill.
  - Resizable left **Library** sidebar with search filter, collapsible `My Computer` tree, OS badges, status dots, and right-click context menu.
  - Multi-tab workspace (`Home` + open VM tabs).
  - **Home tab**: Clean 2-card Quick Start + equal-height **Virtualization Hardware** & **Host Resources** cards.
  - **VM Summary tab**: Clean header with **Power On**, balanced 2-column **Virtual Hardware Devices** table (every row clickable!) & **Management & Storage** card.
  - **Embedded 60fps GPU VNC Console**: Direct `TextureHandle::set_partial` framebuffer inside the VM tab, `Shift+F10` (QMP), `Ctrl+Alt+Del` (QMP), `1:1 Native / Scale Fit`, `Fullscreen` (`Ctrl+Alt+Enter` with floating top hover bar), and `Power Off`.
  - **Modals**: New VM Wizard (with native Windows ISO picker & Windows 10/11 auto-UEFI preset), VMware-style two-pane **Virtual Hardware Settings** (all 9 hardware subsystems), and **Snapshot Manager**.
- **[src/qemu.rs](src/qemu.rs)** — QEMU 11.1.0 + WHPX + QMP + `qemu-img` backend:
  - Stores VMs in `%USERPROFILE%\Documents\BoxVMs\<vm-id>\` (`vm.json`, `disk.qcow2`, `efivars.qcow2`).
  - Supports Q35 PCIe / i440FX, SeaBIOS / OVMF UEFI, VirtIO / SATA AHCI / IDE disks, xHCI USB 3.0 / EHCI USB 2.0 + `usb-tablet`, Intel HDA / AC'97 DirectSound audio, e1000 / VirtIO / RTL8139 NAT NIC, Standard / VirtIO / QXL VGA.
  - Live WHPX disk snapshots via QMP `blockdev-snapshot-internal-sync` / `blockdev-snapshot-delete-internal-sync` + offline `qemu-img snapshot`.
- **[src/vnc.rs](src/vnc.rs)** — Zero-dependency pure-Rust TCP RFB 3.8 VNC client on a background thread with event-driven `ctx.request_repaint()` (0% idle CPU), forced alpha=255 on RGBX pixels, `DesktopSize` dynamic resizing, and bounds-checked `set_partial` dirty-rect GPU updates.
- **[src/models.rs](src/models.rs)** — Serde data models for VM configs, hardware settings, snapshots, engine info, and host stats.
- **[ROASTS.md](ROASTS.md)** — Immortalized double roast of the React/TypeScript/Electron cult and the Base44/Lovable/Bolt.new vibe-coding ecosystem.

---

## 2. How the User Relentlessly Bullied the Agent Into the Exact UI/UX Design They Wanted (And Why Every Punch Was Right)

Future agents reading this: **DO NOT REGRESS ANY OF THESE DECISIONS.** The user did not politely accept mediocre UI—they relentlessly roasted, butchered, and bullied the agent through multiple iterations until the UI/UX actually looked and behaved like real flagship native workstation software:

1. **Nuked VirtualBox:** Started by cloning VirtualBox, saw its 3,989 crusty 2009 PNGs and diagonal `QLinearGradient` `QGraphicsScene` slop, and immediately ordered: *"unclone VirtualBox, let's build our own called Box."*
2. **Nuked React + TypeScript + Tauri:** The first prototype used Tauri + React. The user took one look at it, told the agent straight up *"The visuals look disgusting. They honestly do,"* refused to pretend a WebView is a native desktop app, and forced a **100% pure-Rust rewrite** after dispatching a research subagent (`eframe` / `egui` 0.37 on DX12 `wgpu`).
3. **Relentlessly Butchered the First Pure-Rust UI Until It Was Actually Clean:**
   - First pure-Rust pass was **too huge for the screen** (`1320×820` overflowing Windows DPI scaling), **missing virtualization hardware controls**, and **visibly janky** → user called it out immediately, forcing `1040×660` (min `760×480`), clean `ui.columns` responsive layout, and zero-warning builds.
   - Next pass STILL had **cluttered SaaS hero banners, duplicate "+ New VM" buttons, uneven overlapping cards, invalid premultiplied-alpha neon-cyan blobs, and Unicode tofu rectangles (`▯`)** where Segoe UI lacked media glyphs → user dropped a screenshot and asked *"It's really, really cluttered. You see a bunch of problems here, right?"*
   - Then the user killed dark mode outright (*"can we have only light mode?"*) → forced a crisp **Light Mode ONLY** Windows 11 / VMware Workstation aesthetic (`#FFFFFF` cards on `#F3F5F8` canvas, hand-painted vector icons in `theme.rs`, symmetrical card heights, zero visual fluff).
4. **Demanded Real Virtualization Hardware & Ergonomics:**
   - Restored a full **VMware-style 9-device Hardware Settings** two-pane dialog and clickable Devices table.
   - Added a one-click **`Shift+F10`** button (QMP `send-key`) because laptop media keys hijack `F10` during Windows 11 Setup.
   - Added true **Fullscreen Console Mode** (`Ctrl+Alt+Enter` + top hover pill).
5. **Battle-Tested Against a Live Windows 11 26300 ISO:** The user didn't stop at a pretty UI—they booted real Windows 11, deliberately crashed Setup, nuked the Setup/OOBE keys in `regedit`, and reached the live `box` Windows 11 desktop.

---

## 3. Critical Technical Gotchas (Do NOT Break These!)

1. **Windows `System32\en-us` Keymap Collision:** Always set `cmd.current_dir(qemu_dir)` (`C:\Program Files\qemu`) and `-L C:\Program Files\qemu\share` when spawning QEMU. If Box is launched from `C:\Windows\System32`, QEMU's `-vnc` tries to open `C:\Windows\System32\en-us` (a directory!) as a keymap file and aborts with `Permission denied`.
2. **WHPX Performance & Windows 11 Stability:**
   - Always use **`-accel whpx`** (hardware APIC / `kernel-irqchip=on`). NEVER revert to `kernel-irqchip=off` (causes 10–20× slowdown and Windows 11 boot hangs).
   - For Windows guests under WHPX, always pass `-cpu host,hv_relaxed,hv_spinlocks=0x1fff,hv_vapic,hv_time`, single-socket topology `-smp N,sockets=1,cores=N,threads=1`, `-rtc base=localtime,clock=host`, and `cache=writeback` on `disk.qcow2`.
3. **WHPX Snapshots & Hard Reset:**
   - QEMU's WHPX backend blocks full RAM migration (`savevm`) and freezes vCPUs on QMP `system_reset`.
   - Live WHPX snapshots MUST use QMP `blockdev-snapshot-internal-sync` / `blockdev-snapshot-delete-internal-sync` on `id=disk0`, and `qemu-img snapshot -U -l` (`-U` force-share) to list snapshots while running.
   - WHPX **Reset** MUST perform a clean QEMU power-cycle (`force_stop_vm` → `start_vm`) and reconnect `VncClient`.
   - UEFI NVRAM must be converted to **`efivars.qcow2`** (`format=qcow2`), not `raw`.
4. **VNC Input & Framebuffer:**
   - NEVER map `Key::Space` (or other printable characters) in `egui_key_to_keysym` — `egui` emits `Event::Text(" ")` for printable chars, so mapping `Key::Space` causes double-spacing.
   - Always force alpha byte to `255` on 32-bit VNC RGBX pixels and bounds-check `TextureHandle::set_partial` to avoid panics on resolution switches.
5. **Launching on Windows Desktop:**
   - Build: `cargo build` (or `cargo build --release`).
   - When launching from an agent session onto the interactive desktop (including on battery power), use PowerShell `Register-ScheduledTask` with `-AllowStartIfOnBatteries -DontStopIfGoingOnBatteries`.
