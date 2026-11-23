use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VmStatus {
    Stopped,
    Running,
    Paused,
}

fn default_machine() -> String {
    "q35".into()
}
fn default_firmware() -> String {
    "bios".into()
}
fn default_usb() -> String {
    "xhci".into()
}
fn default_audio() -> String {
    "intel-hda".into()
}
fn default_vram() -> u32 {
    128
}
fn default_disk_bus() -> String {
    "virtio".into()
}
fn default_net_model() -> String {
    "e1000".into()
}
fn default_display_res() -> String {
    "1920x1080".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub id: String,
    pub name: String,
    pub os_type: String,
    pub ram_mb: u32,
    pub cpus: u32,
    pub disk_gb: u32,
    pub disk_path: String,
    pub iso_path: Option<String>,
    pub accel: String,      // auto | whpx | tcg
    pub vga: String,        // std | virtio | qxl
    pub network: bool,
    pub boot_order: String, // dc | cd
    pub notes: String,
    pub created_at: String,

    // Virtualization Hardware Fields
    #[serde(default = "default_machine")]
    pub machine_type: String, // q35 | pc
    #[serde(default = "default_firmware")]
    pub firmware: String, // bios | uefi
    #[serde(default = "default_usb")]
    pub usb_controller: String, // xhci | ehci | none
    #[serde(default = "default_audio")]
    pub audio: String, // intel-hda | ac97 | none
    #[serde(default = "default_vram")]
    pub vram_mb: u32,
    #[serde(default = "default_disk_bus")]
    pub disk_bus: String, // virtio | sata | ide
    #[serde(default = "default_net_model")]
    pub net_model: String, // e1000 | virtio-net-pci | rtl8139
    #[serde(default = "default_display_res")]
    pub display_res: String, // 1920x1080 | 1600x900 | 1366x768 | 1280x720 | 2560x1440 | 1280x800
}

#[derive(Debug, Clone)]
pub struct VmInfo {
    pub config: VmConfig,
    pub status: VmStatus,
    pub vnc_port: Option<u16>,
    pub accel_active: Option<String>,
    pub disk_used_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct CreateVmRequest {
    pub name: String,
    pub os_type: String,
    pub ram_mb: u32,
    pub cpus: u32,
    pub disk_gb: u32,
    pub iso_path: Option<String>,
    pub accel: String,
    pub network: bool,
    pub firmware: String,
    pub usb_controller: String,
    pub audio: String,
    pub notes: String,
}

#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub id: String,
    pub name: String,
    pub vm_size: String,
    pub date: String,
}

#[derive(Debug, Clone)]
pub struct EngineInfo {
    pub qemu_found: bool,
    pub qemu_version: String,
    pub vms_dir: String,
    pub cpu_brand: String,
    pub ovmf_found: bool,
}

#[derive(Debug, Clone)]
pub struct HostStats {
    pub cpu_usage: f32,
    pub cpu_cores: usize,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
}
