use crate::models::{
    CreateVmRequest, EngineInfo, HostStats, SnapshotInfo, VmConfig, VmInfo, VmStatus,
};
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use sysinfo::System;
use uuid::Uuid;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct RunningVm {
    pub child: Child,
    pub vnc_port: u16,
    pub qmp_port: u16,
    pub status: VmStatus,
    pub accel_active: String,
    pub stderr_log: Arc<Mutex<String>>,
}

pub struct VmManager {
    pub vms_dir: PathBuf,
    pub running: HashMap<String, RunningVm>,
    pub sys: System,
    pub last_exit_error: Option<String>,
}

impl VmManager {
    pub fn new() -> Self {
        let base = dirs::document_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("BoxVMs");
        let _ = fs::create_dir_all(&base);
        let mut sys = System::new_all();
        sys.refresh_all();
        Self {
            vms_dir: base,
            running: HashMap::new(),
            sys,
            last_exit_error: None,
        }
    }

    pub fn take_last_exit_error(&mut self) -> Option<String> {
        self.last_exit_error.take()
    }

    pub fn find_qemu_bin(&self, name: &str) -> Option<PathBuf> {
        let candidates = [
            PathBuf::from(format!("C:\\Program Files\\qemu\\{name}.exe")),
            PathBuf::from(format!("C:\\Program Files (x86)\\qemu\\{name}.exe")),
            PathBuf::from(format!("{name}.exe")),
            PathBuf::from(name),
        ];
        for c in candidates {
            if c.exists() {
                return Some(c);
            }
        }
        let mut cmd = Command::new("where");
        cmd.arg(name);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                if let Some(first) = s.lines().next() {
                    let p = PathBuf::from(first.trim());
                    if p.exists() {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    pub fn find_ovmf_code(&self) -> Option<PathBuf> {
        let candidates = [
            PathBuf::from("C:\\Program Files\\qemu\\share\\edk2-x86_64-code.fd"),
            PathBuf::from("C:\\Program Files\\qemu\\share\\OVMF.fd"),
        ];
        candidates.into_iter().find(|p| p.exists())
    }

    pub fn find_ovmf_vars_template(&self) -> Option<PathBuf> {
        let candidates = [
            PathBuf::from("C:\\Program Files\\qemu\\share\\edk2-i386-vars.fd"),
            PathBuf::from("C:\\Program Files\\qemu\\share\\edk2-x86_64-vars.fd"),
        ];
        candidates.into_iter().find(|p| p.exists())
    }

    pub fn engine_info(&self) -> EngineInfo {
        let qemu = self.find_qemu_bin("qemu-system-x86_64");
        let version = if let Some(ref q) = qemu {
            let mut cmd = Command::new(q);
            cmd.arg("--version");
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd.output()
                .ok()
                .and_then(|o| {
                    let txt = String::from_utf8_lossy(&o.stdout);
                    txt.lines().next().map(|l| {
                        let s = l.trim();
                        if let Some(idx) = s.find('(') {
                            s[..idx].trim().replace("emulator version ", "")
                        } else {
                            s.replace("emulator version ", "")
                        }
                    })
                })
                .unwrap_or_else(|| "QEMU detected".to_string())
        } else {
            "QEMU not found".to_string()
        };

        let cpu_brand = self
            .sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "x86_64 Host CPU".to_string());

        EngineInfo {
            qemu_found: qemu.is_some(),
            qemu_version: version,
            vms_dir: self.vms_dir.to_string_lossy().to_string(),
            cpu_brand,
            ovmf_found: self.find_ovmf_code().is_some(),
        }
    }

    pub fn host_stats(&mut self) -> HostStats {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        HostStats {
            cpu_usage: self.sys.global_cpu_usage(),
            cpu_cores: self.sys.cpus().len(),
            ram_total_gb: self.sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0,
            ram_used_gb: self.sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0,
        }
    }

    fn vm_dir(&self, id: &str) -> PathBuf {
        self.vms_dir.join(id)
    }

    pub fn load_config(&self, id: &str) -> Result<VmConfig, String> {
        let raw = fs::read_to_string(self.vm_dir(id).join("vm.json"))
            .map_err(|e| format!("Failed to read VM config: {e}"))?;
        serde_json::from_str(&raw).map_err(|e| format!("Invalid VM config: {e}"))
    }

    pub fn save_config(&self, cfg: &VmConfig) -> Result<(), String> {
        let dir = self.vm_dir(&cfg.id);
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create VM dir: {e}"))?;
        let json = serde_json::to_string_pretty(cfg)
            .map_err(|e| format!("Failed to serialize VM config: {e}"))?;
        fs::write(dir.join("vm.json"), json)
            .map_err(|e| format!("Failed to write VM config: {e}"))
    }

    pub fn cleanup_exited(&mut self) {
        let mut dead = Vec::new();
        for (id, rvm) in self.running.iter_mut() {
            if let Ok(Some(status)) = rvm.child.try_wait() {
                let err_txt = rvm.stderr_log.lock().map(|s| s.trim().to_string()).unwrap_or_default();
                if !status.success() {
                    let msg = if err_txt.is_empty() {
                        format!("QEMU exited unexpectedly ({status})")
                    } else {
                        format!("QEMU exited ({status}): {err_txt}")
                    };
                    self.last_exit_error = Some(msg);
                }
                dead.push(id.clone());
            }
        }
        for id in dead {
            self.running.remove(&id);
        }
    }

    pub fn list_vms(&mut self) -> Vec<VmInfo> {
        self.cleanup_exited();
        let mut list = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.vms_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let cfg_path = entry.path().join("vm.json");
                    if cfg_path.exists() {
                        if let Ok(raw) = fs::read_to_string(&cfg_path) {
                            if let Ok(cfg) = serde_json::from_str::<VmConfig>(&raw) {
                                list.push(self.build_vm_info(cfg));
                            }
                        }
                    }
                }
            }
        }
        list.sort_by(|a, b| a.config.name.to_lowercase().cmp(&b.config.name.to_lowercase()));
        list
    }

    fn build_vm_info(&self, config: VmConfig) -> VmInfo {
        let disk_used_bytes = fs::metadata(&config.disk_path).map(|m| m.len()).unwrap_or(0);
        if let Some(rvm) = self.running.get(&config.id) {
            VmInfo {
                config,
                status: rvm.status.clone(),
                vnc_port: Some(rvm.vnc_port),
                accel_active: Some(rvm.accel_active.clone()),
                disk_used_bytes,
            }
        } else {
            VmInfo {
                config,
                status: VmStatus::Stopped,
                vnc_port: None,
                accel_active: None,
                disk_used_bytes,
            }
        }
    }

    pub fn create_vm(&mut self, req: CreateVmRequest) -> Result<VmInfo, String> {
        let qemu_img = self
            .find_qemu_bin("qemu-img")
            .ok_or_else(|| "qemu-img.exe not found.".to_string())?;

        let id = Uuid::new_v4().to_string();
        let dir = self.vm_dir(&id);
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create VM dir: {e}"))?;

        let disk_path = dir.join("disk.qcow2");
        let mut cmd = Command::new(&qemu_img);
        cmd.args([
            "create",
            "-f",
            "qcow2",
            &disk_path.to_string_lossy(),
            &format!("{}G", req.disk_gb.max(1)),
        ]);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = cmd
            .output()
            .map_err(|e| format!("Failed to execute qemu-img: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "qemu-img failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }

        let boot_order = if req.iso_path.as_ref().is_some_and(|s| !s.trim().is_empty()) {
            "dc".to_string()
        } else {
            "cd".to_string()
        };

        let disk_bus = if req.os_type == "windows" {
            "sata".to_string()
        } else {
            "virtio".to_string()
        };

        let cfg = VmConfig {
            id,
            name: req.name.trim().to_string(),
            os_type: req.os_type,
            ram_mb: req.ram_mb.max(256),
            cpus: req.cpus.max(1),
            disk_gb: req.disk_gb.max(1),
            disk_path: disk_path.to_string_lossy().to_string(),
            iso_path: req.iso_path.filter(|s| !s.trim().is_empty()),
            accel: req.accel,
            vga: "std".to_string(),
            network: req.network,
            boot_order,
            notes: req.notes,
            created_at: Utc::now().to_rfc3339(),
            machine_type: "q35".to_string(),
            firmware: req.firmware,
            usb_controller: req.usb_controller,
            audio: req.audio,
            vram_mb: 128,
            disk_bus,
            net_model: "e1000".to_string(),
            display_res: "1920x1080".to_string(),
        };

        self.save_config(&cfg)?;
        Ok(self.build_vm_info(cfg))
    }

    pub fn update_vm(&mut self, cfg: VmConfig) -> Result<VmInfo, String> {
        self.save_config(&cfg)?;
        Ok(self.build_vm_info(cfg))
    }

    pub fn delete_vm(&mut self, id: &str) -> Result<(), String> {
        self.force_stop_vm(id);
        let dir = self.vm_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| format!("Failed to delete VM dir: {e}"))?;
        }
        Ok(())
    }

    fn pick_free_port(start: u16, end: u16) -> Result<u16, String> {
        for p in start..=end {
            if TcpListener::bind(("127.0.0.1", p)).is_ok() {
                return Ok(p);
            }
        }
        Err("No free TCP port available".to_string())
    }

    fn spawn_qemu(
        &self,
        qemu_bin: &Path,
        cfg: &VmConfig,
        vnc_display: u16,
        qmp_port: u16,
        accel_mode: &str,
    ) -> Result<(Child, Arc<Mutex<String>>), String> {
        let mut cmd = Command::new(qemu_bin);
        // CRITICAL on Windows: if Box is launched from System32 (which has an `en-us/` folder),
        // QEMU's -vnc tries to open `./en-us` as a keymap file and fails with Permission denied.
        if let Some(qemu_dir) = qemu_bin.parent() {
            cmd.current_dir(qemu_dir);
            let share_dir = qemu_dir.join("share");
            if share_dir.exists() {
                cmd.args(["-L", &share_dir.to_string_lossy()]);
            }
        }
        cmd.args(["-name", &cfg.name]);
        let is_q35 = cfg.machine_type != "pc";
        cmd.args(["-machine", if is_q35 { "q35" } else { "pc" }]);
        cmd.args(["-m", &cfg.ram_mb.to_string()]);
        // Single-socket multi-core topology (Windows limits physical sockets!)
        cmd.args([
            "-smp",
            &format!("{},sockets=1,cores={},threads=1", cfg.cpus, cfg.cpus),
        ]);

        if accel_mode == "whpx" {
            // Hardware APIC (kernel-irqchip=on) — 10x+ faster than kernel-irqchip=off and prevents Windows hangs
            cmd.args(["-accel", "whpx"]);
            if cfg.os_type == "windows" {
                cmd.args([
                    "-cpu",
                    "host,hv_relaxed,hv_spinlocks=0x1fff,hv_vapic,hv_time",
                ]);
            } else {
                cmd.args(["-cpu", "host"]);
            }
        } else {
            cmd.args(["-accel", "tcg"]);
            cmd.args(["-cpu", "max"]);
        }

        if cfg.os_type == "windows" {
            cmd.args(["-rtc", "base=localtime,clock=host"]);
        }

        // UEFI Firmware (OVMF code + per-VM writable QCOW2 NVRAM so live `savevm` snapshots work!)
        if cfg.firmware == "uefi" {
            if let Some(ovmf_code) = self.find_ovmf_code() {
                cmd.args([
                    "-drive",
                    &format!("if=pflash,format=raw,readonly=on,file={}", ovmf_code.display()),
                ]);
                let vm_vars_qcow2 = self.vm_dir(&cfg.id).join("efivars.qcow2");
                let vm_vars_raw = self.vm_dir(&cfg.id).join("efivars.fd");
                if !vm_vars_qcow2.exists() {
                    if let Some(qemu_img) = self.find_qemu_bin("qemu-img") {
                        let src = if vm_vars_raw.exists() {
                            Some(vm_vars_raw.clone())
                        } else {
                            self.find_ovmf_vars_template()
                        };
                        if let Some(src_path) = src {
                            let mut conv = Command::new(&qemu_img);
                            conv.args([
                                "convert",
                                "-f",
                                "raw",
                                "-O",
                                "qcow2",
                                &src_path.to_string_lossy(),
                                &vm_vars_qcow2.to_string_lossy(),
                            ]);
                            #[cfg(windows)]
                            conv.creation_flags(CREATE_NO_WINDOW);
                            let _ = conv.output();
                        }
                    }
                }
                if vm_vars_qcow2.exists() {
                    cmd.args([
                        "-drive",
                        &format!("if=pflash,format=qcow2,file={}", vm_vars_qcow2.display()),
                    ]);
                } else if vm_vars_raw.exists() {
                    cmd.args([
                        "-drive",
                        &format!("if=pflash,format=raw,file={}", vm_vars_raw.display()),
                    ]);
                }
            }
        }

        // Storage Controller + Disk (uses q35 built-in AHCI ide.0 for SATA — zero conflicts!)
        let escaped_disk = cfg.disk_path.replace(',', ",,");
        match cfg.disk_bus.as_str() {
            "sata" => {
                cmd.args([
                    "-drive",
                    &format!("id=disk0,file={escaped_disk},format=qcow2,if=none,cache=writeback"),
                ]);
                if is_q35 {
                    cmd.args(["-device", "ide-hd,drive=disk0,bus=ide.0"]);
                } else {
                    cmd.args(["-device", "ahci,id=ahci"]);
                    cmd.args(["-device", "ide-hd,drive=disk0,bus=ahci.0"]);
                }
            }
            "ide" => {
                cmd.args([
                    "-drive",
                    &format!("id=disk0,file={escaped_disk},format=qcow2,if=ide,cache=writeback"),
                ]);
            }
            _ => {
                cmd.args([
                    "-drive",
                    &format!("id=disk0,file={escaped_disk},format=qcow2,if=virtio,cache=writeback"),
                ]);
            }
        }

        // CD/DVD ISO mounted cleanly on ide.1 (or -cdrom on legacy pc)
        if let Some(ref iso) = cfg.iso_path {
            if Path::new(iso).exists() {
                let escaped_iso = iso.replace(',', ",,");
                if is_q35 {
                    cmd.args([
                        "-drive",
                        &format!("id=cd0,file={escaped_iso},media=cdrom,if=none"),
                    ]);
                    cmd.args(["-device", "ide-cd,drive=cd0,bus=ide.1"]);
                } else {
                    cmd.args(["-cdrom", iso]);
                }
            }
        }

        cmd.args(["-boot", &format!("order={}", cfg.boot_order)]);

        // Display / GPU with EDID native resolution advertising
        let (xres, yres) = cfg
            .display_res
            .split_once('x')
            .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
            .unwrap_or((1920, 1080));
        let vram = cfg.vram_mb.clamp(16, 256);
        cmd.args(["-vga", "none"]);
        match cfg.vga.as_str() {
            "virtio" => {
                cmd.args([
                    "-device",
                    &format!("virtio-vga,edid=on,xres={xres},yres={yres}"),
                ]);
            }
            "qxl" => {
                cmd.args([
                    "-device",
                    &format!("qxl-vga,vgamem_mb={vram}"),
                ]);
            }
            _ => {
                cmd.args([
                    "-device",
                    &format!("VGA,edid=on,xres={xres},yres={yres},vgamem_mb={vram}"),
                ]);
            }
        }

        // USB Controller + Absolute Tablet Pointer
        match cfg.usb_controller.as_str() {
            "ehci" => {
                cmd.args(["-device", "usb-ehci", "-device", "usb-tablet"]);
            }
            "none" => {}
            _ => {
                cmd.args(["-device", "qemu-xhci", "-device", "usb-tablet"]);
            }
        }

        // Sound Card with explicit Windows DirectSound backend
        match cfg.audio.as_str() {
            "intel-hda" => {
                cmd.args([
                    "-audiodev",
                    "dsound,id=snd0",
                    "-device",
                    "intel-hda",
                    "-device",
                    "hda-duplex,audiodev=snd0",
                ]);
            }
            "ac97" => {
                cmd.args(["-audiodev", "dsound,id=snd0", "-device", "AC97,audiodev=snd0"]);
            }
            _ => {}
        }

        // Network Adapter
        if cfg.network {
            let nic = match cfg.net_model.as_str() {
                "virtio-net-pci" => "virtio-net-pci",
                "rtl8139" => "rtl8139",
                _ => "e1000",
            };
            cmd.args([
                "-netdev",
                "user,id=net0",
                "-device",
                &format!("{nic},netdev=net0"),
            ]);
        } else {
            cmd.args(["-nic", "none"]);
        }

        cmd.args(["-vnc", &format!("127.0.0.1:{vnc_display}")]);
        cmd.args([
            "-qmp",
            &format!("tcp:127.0.0.1:{qmp_port},server=on,wait=off"),
        ]);

        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let mut child = cmd.spawn().map_err(|e| format!("Failed to start QEMU: {e}"))?;
        let stderr_log = Arc::new(Mutex::new(String::new()));
        if let Some(mut se) = child.stderr.take() {
            let log_ref = stderr_log.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = se.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let chunk = String::from_utf8_lossy(&buf[..n]);
                    if let Ok(mut s) = log_ref.lock() {
                        s.push_str(&chunk);
                    }
                }
            });
        }
        Ok((child, stderr_log))
    }

    pub fn start_vm(&mut self, id: &str) -> Result<VmInfo, String> {
        self.cleanup_exited();
        self.last_exit_error = None;
        if self.running.contains_key(id) {
            let cfg = self.load_config(id)?;
            return Ok(self.build_vm_info(cfg));
        }

        let cfg = self.load_config(id)?;
        let qemu_bin = self
            .find_qemu_bin("qemu-system-x86_64")
            .ok_or_else(|| "qemu-system-x86_64.exe not found.".to_string())?;

        let vnc_port = Self::pick_free_port(5901, 5999)?;
        let vnc_display = vnc_port - 5900;
        let qmp_port = Self::pick_free_port(4440, 4599)?;

        let try_whpx = cfg.accel == "whpx" || cfg.accel == "auto";
        let mut active_accel = if try_whpx { "whpx" } else { "tcg" };
        let (mut child, mut stderr_log) =
            self.spawn_qemu(&qemu_bin, &cfg, vnc_display, qmp_port, active_accel)?;

        std::thread::sleep(Duration::from_millis(500));
        if let Ok(Some(st)) = child.try_wait() {
            if cfg.accel == "auto" && active_accel == "whpx" {
                active_accel = "tcg";
                let (c2, log2) =
                    self.spawn_qemu(&qemu_bin, &cfg, vnc_display, qmp_port, active_accel)?;
                child = c2;
                stderr_log = log2;
                std::thread::sleep(Duration::from_millis(400));
                if let Ok(Some(st2)) = child.try_wait() {
                    let err_txt = stderr_log.lock().map(|s| s.trim().to_string()).unwrap_or_default();
                    return Err(if err_txt.is_empty() {
                        format!("QEMU exited immediately ({st2})")
                    } else {
                        err_txt
                    });
                }
            } else {
                let err_txt = stderr_log.lock().map(|s| s.trim().to_string()).unwrap_or_default();
                return Err(if err_txt.is_empty() {
                    format!("QEMU exited during startup ({st})")
                } else {
                    err_txt
                });
            }
        }

        self.running.insert(
            id.to_string(),
            RunningVm {
                child,
                vnc_port,
                qmp_port,
                status: VmStatus::Running,
                accel_active: active_accel.to_uppercase(),
                stderr_log,
            },
        );

        Ok(self.build_vm_info(cfg))
    }

    fn send_qmp(&self, qmp_port: u16, cmd_json: &str) -> Result<String, String> {
        let mut stream = TcpStream::connect(("127.0.0.1", qmp_port))
            .map_err(|e| format!("QMP connect failed: {e}"))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

        let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
        let mut line = String::new();
        // 1. Read QMP greeting
        reader
            .read_line(&mut line)
            .map_err(|e| format!("QMP greeting failed: {e}"))?;

        // 2. Negotiate capabilities
        stream
            .write_all(b"{\"execute\":\"qmp_capabilities\"}\r\n")
            .map_err(|e| e.to_string())?;
        let _ = stream.flush();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).map_err(|e| e.to_string())?;
            if n == 0 || line.contains("\"return\"") || line.contains("\"error\"") {
                break;
            }
        }

        // 3. Execute command and wait for its actual return/error (skipping async events)
        stream
            .write_all(format!("{cmd_json}\r\n").as_bytes())
            .map_err(|e| e.to_string())?;
        let _ = stream.flush();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("QMP connection closed before response".to_string());
            }
            if line.contains("\"error\"") {
                return Err(format!("QMP error: {}", line.trim()));
            }
            if line.contains("\"return\"") {
                return Ok(line);
            }
        }
    }

    pub fn stop_vm(&mut self, id: &str, force: bool) {
        if !force {
            if let Some(r) = self.running.get(id) {
                let _ = self.send_qmp(r.qmp_port, r#"{"execute":"system_powerdown"}"#);
                std::thread::sleep(Duration::from_millis(450));
            }
        }
        self.force_stop_vm(id);
    }

    pub fn force_stop_vm(&mut self, id: &str) {
        if let Some(mut rvm) = self.running.remove(id) {
            let _ = rvm.child.kill();
            let _ = rvm.child.wait();
        }
    }

    pub fn toggle_pause(&mut self, id: &str) -> Result<(), String> {
        let (qmp_port, is_paused) = match self.running.get(id) {
            Some(r) => (r.qmp_port, r.status == VmStatus::Paused),
            None => return Err("VM is not running".to_string()),
        };
        let cmd = if is_paused {
            r#"{"execute":"cont"}"#
        } else {
            r#"{"execute":"stop"}"#
        };
        self.send_qmp(qmp_port, cmd)?;
        if let Some(r) = self.running.get_mut(id) {
            r.status = if is_paused {
                VmStatus::Running
            } else {
                VmStatus::Paused
            };
        }
        Ok(())
    }

    pub fn reset_vm(&mut self, id: &str) -> Result<Option<u16>, String> {
        let (port, is_whpx) = self
            .running
            .get(id)
            .map(|r| (r.qmp_port, r.accel_active == "WHPX"))
            .ok_or_else(|| "VM is not running".to_string())?;
        if is_whpx {
            // WHPX does not cleanly reset vCPU/APIC registers on QMP `system_reset` (causes freeze).
            // Perform a clean hard power-cycle of QEMU and return the new VNC port to reconnect.
            self.force_stop_vm(id);
            let info = self.start_vm(id)?;
            return Ok(info.vnc_port);
        }
        self.send_qmp(port, r#"{"execute":"system_reset"}"#)?;
        Ok(None)
    }

    pub fn send_ctrl_alt_del(&self, id: &str) -> Result<(), String> {
        let port = self
            .running
            .get(id)
            .map(|r| r.qmp_port)
            .ok_or_else(|| "VM is not running".to_string())?;
        self.send_qmp(
            port,
            r#"{"execute":"send-key","arguments":{"keys":[{"type":"qcode","data":"ctrl"},{"type":"qcode","data":"alt"},{"type":"qcode","data":"delete"}]}}"#,
        )?;
        Ok(())
    }

    pub fn send_shift_f10(&self, id: &str) -> Result<(), String> {
        let port = self
            .running
            .get(id)
            .map(|r| r.qmp_port)
            .ok_or_else(|| "VM is not running".to_string())?;
        self.send_qmp(
            port,
            r#"{"execute":"send-key","arguments":{"keys":[{"type":"qcode","data":"shift"},{"type":"qcode","data":"f10"}]}}"#,
        )?;
        Ok(())
    }

    pub fn list_snapshots(&self, id: &str) -> Result<Vec<SnapshotInfo>, String> {
        let cfg = self.load_config(id)?;
        let qemu_img = self
            .find_qemu_bin("qemu-img")
            .ok_or_else(|| "qemu-img not found".to_string())?;
        let mut cmd = Command::new(&qemu_img);
        // -U (force-share) allows listing snapshots even while QEMU holds the disk lock!
        cmd.args(["snapshot", "-U", "-l", &cfg.disk_path]);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = cmd.output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            if !err.is_empty() {
                return Err(err);
            }
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut snaps = Vec::new();
        for line in text.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                snaps.push(SnapshotInfo {
                    id: parts[0].to_string(),
                    name: parts[1].to_string(),
                    vm_size: parts[2].to_string(),
                    date: format!("{} {}", parts[3], parts[4]),
                });
            }
        }
        Ok(snaps)
    }

    pub fn create_snapshot(&self, id: &str, name: &str) -> Result<(), String> {
        let clean = name.trim().replace(' ', "_");
        if clean.is_empty() {
            return Err("Snapshot name cannot be empty".to_string());
        }
        if let Some(r) = self.running.get(id) {
            if r.accel_active == "WHPX" {
                // WHPX blocks RAM migration (`savevm`), so use QMP live internal QCOW2 disk snapshot!
                let cmd = format!(
                    r#"{{"execute":"blockdev-snapshot-internal-sync","arguments":{{"device":"disk0","name":"{clean}"}}}}"#
                );
                self.send_qmp(r.qmp_port, &cmd)?;
                return Ok(());
            }
            let cmd = format!(
                r#"{{"execute":"human-monitor-command","arguments":{{"command-line":"savevm {clean}"}}}}"#
            );
            let resp = self.send_qmp(r.qmp_port, &cmd)?;
            if resp.contains("Error:") || resp.contains("does not support") {
                return Err(resp.trim().to_string());
            }
            return Ok(());
        }
        let cfg = self.load_config(id)?;
        let qemu_img = self
            .find_qemu_bin("qemu-img")
            .ok_or_else(|| "qemu-img not found".to_string())?;
        let mut cmd = Command::new(&qemu_img);
        cmd.args(["snapshot", "-c", &clean, &cfg.disk_path]);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = cmd.output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).to_string());
        }
        Ok(())
    }

    pub fn restore_snapshot(&self, id: &str, name: &str) -> Result<(), String> {
        if let Some(r) = self.running.get(id) {
            if r.accel_active == "WHPX" {
                return Err(
                    "Power off the VM first to revert a disk snapshot under WHPX acceleration."
                        .to_string(),
                );
            }
            let cmd = format!(
                r#"{{"execute":"human-monitor-command","arguments":{{"command-line":"loadvm {name}"}}}}"#
            );
            let resp = self.send_qmp(r.qmp_port, &cmd)?;
            if resp.contains("Error:") {
                return Err(resp.trim().to_string());
            }
            return Ok(());
        }
        let cfg = self.load_config(id)?;
        let qemu_img = self
            .find_qemu_bin("qemu-img")
            .ok_or_else(|| "qemu-img not found".to_string())?;
        let mut cmd = Command::new(&qemu_img);
        cmd.args(["snapshot", "-a", name, &cfg.disk_path]);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = cmd.output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).to_string());
        }
        Ok(())
    }

    pub fn delete_snapshot(&self, id: &str, name: &str) -> Result<(), String> {
        if let Some(r) = self.running.get(id) {
            if r.accel_active == "WHPX" {
                let cmd = format!(
                    r#"{{"execute":"blockdev-snapshot-delete-internal-sync","arguments":{{"device":"disk0","name":"{name}"}}}}"#
                );
                self.send_qmp(r.qmp_port, &cmd)?;
                return Ok(());
            }
            let cmd = format!(
                r#"{{"execute":"human-monitor-command","arguments":{{"command-line":"delvm {name}"}}}}"#
            );
            let resp = self.send_qmp(r.qmp_port, &cmd)?;
            if resp.contains("Error:") {
                return Err(resp.trim().to_string());
            }
            return Ok(());
        }
        let cfg = self.load_config(id)?;
        let qemu_img = self
            .find_qemu_bin("qemu-img")
            .ok_or_else(|| "qemu-img not found".to_string())?;
        let mut cmd = Command::new(&qemu_img);
        cmd.args(["snapshot", "-d", name, &cfg.disk_path]);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = cmd.output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).to_string());
        }
        Ok(())
    }

    pub fn open_vm_folder(&self, id: &str) {
        let _ = Command::new("explorer").arg(self.vm_dir(id)).spawn();
    }

    pub fn shutdown_all(&mut self) {
        let ids: Vec<String> = self.running.keys().cloned().collect();
        for id in ids {
            self.force_stop_vm(&id);
        }
    }
}
