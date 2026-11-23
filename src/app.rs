use crate::models::{CreateVmRequest, EngineInfo, HostStats, SnapshotInfo, VmConfig, VmInfo, VmStatus};
use crate::qemu::VmManager;
use crate::theme::{
    self, IconKind, ACCENT, ACCENT_BG, ACCENT_BORDER, AMBER, AMBER_BG, BG_APP, BG_CARD, BG_CHROME,
    BG_INPUT, BG_SIDEBAR, BG_TOOLBAR, BORDER_STRONG, BORDER_SUBTLE, GREEN, GREEN_BG, GREEN_BTN,
    RED, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};
use crate::vnc::{egui_key_to_keysym, VncClient, VncInput, VncStatus};
use egui::{
    Align, Color32, Context, CornerRadius, CursorIcon, Frame, Key, Layout, Margin, Pos2, Rect,
    ResizeDirection, RichText, ScrollArea, Sense, Stroke, StrokeKind, Vec2, ViewportCommand,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct CreateForm {
    name: String,
    os_type: String,
    ram_mb: u32,
    cpus: u32,
    disk_gb: u32,
    iso_path: String,
    accel: String,
    firmware: String,
    usb_controller: String,
    audio: String,
    network: bool,
    notes: String,
}

impl Default for CreateForm {
    fn default() -> Self {
        Self {
            name: "Ubuntu 24.04".into(),
            os_type: "ubuntu".into(),
            ram_mb: 4096,
            cpus: 2,
            disk_gb: 32,
            iso_path: String::new(),
            accel: "auto".into(),
            firmware: "bios".into(),
            usb_controller: "xhci".into(),
            audio: "intel-hda".into(),
            network: true,
            notes: String::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HardwareTab {
    Memory,
    Processors,
    HardDisk,
    CdDvd,
    Network,
    Usb,
    Audio,
    Display,
    Firmware,
}

const HARDWARE_TABS: [(HardwareTab, &str); 9] = [
    (HardwareTab::Memory, "Memory"),
    (HardwareTab::Processors, "Processors"),
    (HardwareTab::HardDisk, "Hard Disk (QCOW2)"),
    (HardwareTab::CdDvd, "CD/DVD (ISO)"),
    (HardwareTab::Network, "Network Adapter"),
    (HardwareTab::Usb, "USB Controller"),
    (HardwareTab::Audio, "Sound Card"),
    (HardwareTab::Display, "Display / GPU"),
    (HardwareTab::Firmware, "Firmware & Chipset"),
];

pub struct BoxApp {
    manager: VmManager,
    vms: Vec<VmInfo>,
    engine: EngineInfo,
    host_stats: HostStats,
    last_poll: Instant,

    open_tabs: Vec<String>,
    active_tab: String,
    show_sidebar: bool,
    library_expanded: bool,
    search_query: String,

    vnc_clients: HashMap<String, VncClient>,
    scale_fit: bool,
    fullscreen_console: bool,

    show_create_modal: bool,
    create_form: CreateForm,

    settings_cfg: Option<VmConfig>,
    settings_hw_tab: HardwareTab,

    snapshots_vm_id: Option<String>,
    snapshots_list: Vec<SnapshotInfo>,
    new_snapshot_name: String,

    error_banner: Option<String>,
}

impl BoxApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_workstation_theme(&cc.egui_ctx);
        let mut manager = VmManager::new();
        let engine = manager.engine_info();
        let host_stats = manager.host_stats();
        let vms = manager.list_vms();

        Self {
            manager,
            vms,
            engine,
            host_stats,
            last_poll: Instant::now(),
            open_tabs: Vec::new(),
            active_tab: "home".into(),
            show_sidebar: true,
            library_expanded: true,
            search_query: String::new(),
            vnc_clients: HashMap::new(),
            scale_fit: true,
            fullscreen_console: false,
            show_create_modal: false,
            create_form: CreateForm::default(),
            settings_cfg: None,
            settings_hw_tab: HardwareTab::Memory,
            snapshots_vm_id: None,
            snapshots_list: Vec::new(),
            new_snapshot_name: String::new(),
            error_banner: None,
        }
    }

    fn refresh(&mut self) {
        self.vms = self.manager.list_vms();
        if let Some(exit_err) = self.manager.take_last_exit_error() {
            self.error_banner = Some(exit_err);
        }
        self.host_stats = self.manager.host_stats();
        let running_ids: Vec<String> = self
            .vms
            .iter()
            .filter(|v| v.status != VmStatus::Stopped)
            .map(|v| v.config.id.clone())
            .collect();
        self.vnc_clients.retain(|id, _| running_ids.contains(id));
    }

    fn open_vm_tab(&mut self, id: &str) {
        if !self.open_tabs.iter().any(|t| t == id) {
            self.open_tabs.push(id.to_string());
        }
        self.active_tab = id.to_string();
    }

    fn close_vm_tab(&mut self, id: &str) {
        self.open_tabs.retain(|t| t != id);
        if self.active_tab == id {
            self.active_tab = self
                .open_tabs
                .last()
                .cloned()
                .unwrap_or_else(|| "home".to_string());
        }
    }

    fn active_vm(&self) -> Option<VmInfo> {
        if self.active_tab == "home" {
            None
        } else {
            self.vms.iter().find(|v| v.config.id == self.active_tab).cloned()
        }
    }

    fn start_vm(&mut self, id: &str, ctx: &Context) {
        self.error_banner = None;
        match self.manager.start_vm(id) {
            Ok(info) => {
                if let Some(port) = info.vnc_port {
                    self.vnc_clients
                        .insert(id.to_string(), VncClient::connect(port, ctx.clone()));
                }
                self.open_vm_tab(id);
                self.refresh();
            }
            Err(e) => self.error_banner = Some(e),
        }
    }

    fn stop_vm(&mut self, id: &str, force: bool, ctx: &Context) {
        self.vnc_clients.remove(id);
        self.manager.stop_vm(id, force);
        if self.fullscreen_console {
            self.set_fullscreen(false, ctx);
        }
        self.refresh();
    }

    fn set_fullscreen(&mut self, on: bool, ctx: &Context) {
        self.fullscreen_console = on;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(on));
    }

    fn open_settings(&mut self, cfg: VmConfig, tab: HardwareTab) {
        self.settings_cfg = Some(cfg);
        self.settings_hw_tab = tab;
    }

    fn open_snapshots(&mut self, id: &str) {
        self.snapshots_vm_id = Some(id.to_string());
        self.new_snapshot_name.clear();
        match self.manager.list_snapshots(id) {
            Ok(list) => self.snapshots_list = list,
            Err(e) => {
                self.snapshots_list.clear();
                self.error_banner = Some(e);
            }
        }
    }

    fn handle_frameless_resize(&self, ctx: &Context) {
        let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        if is_max {
            return;
        }
        let rect = ctx.viewport_rect();
        let margin = 6.0;
        if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
            let left = pos.x <= rect.left() + margin;
            let right = pos.x >= rect.right() - margin;
            let top = pos.y <= rect.top() + margin;
            let bottom = pos.y >= rect.bottom() - margin;

            let dir = match (left, right, top, bottom) {
                (true, false, true, false) => Some((ResizeDirection::NorthWest, CursorIcon::ResizeNwSe)),
                (false, true, true, false) => Some((ResizeDirection::NorthEast, CursorIcon::ResizeNeSw)),
                (true, false, false, true) => Some((ResizeDirection::SouthWest, CursorIcon::ResizeNeSw)),
                (false, true, false, true) => Some((ResizeDirection::SouthEast, CursorIcon::ResizeNwSe)),
                (true, false, false, false) => Some((ResizeDirection::West, CursorIcon::ResizeHorizontal)),
                (false, true, false, false) => Some((ResizeDirection::East, CursorIcon::ResizeHorizontal)),
                (false, false, true, false) => Some((ResizeDirection::North, CursorIcon::ResizeVertical)),
                (false, false, false, true) => Some((ResizeDirection::South, CursorIcon::ResizeVertical)),
                _ => None,
            };

            if let Some((resize_dir, cursor)) = dir {
                ctx.set_cursor_icon(cursor);
                if ctx.input(|i| i.pointer.primary_pressed()) {
                    ctx.send_viewport_cmd(ViewportCommand::BeginResize(resize_dir));
                }
            }
        }
    }

    fn ui_titlebar(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let active_title = self
            .active_vm()
            .map(|v| format!("{} — Box! Workstation", v.config.name))
            .unwrap_or_else(|| "Box! Workstation".to_string());

        egui::Panel::top("titlebar")
            .exact_size(34.0)
            .frame(
                Frame::NONE
                    .fill(BG_CHROME)
                    .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                    .inner_margin(Margin {
                        left: 10,
                        right: 0,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;

                    // Brand logo
                    let (logo_rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                    theme::paint_box_logo(ui.painter(), logo_rect.center(), 8.0);
                    ui.label(RichText::new("Box!").strong().size(13.0).color(TEXT_PRIMARY));

                    Frame::NONE
                        .fill(ACCENT_BG)
                        .stroke(Stroke::new(1.0, ACCENT_BORDER))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::symmetric(6, 1))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("WORKSTATION")
                                    .size(9.0)
                                    .strong()
                                    .color(ACCENT),
                            );
                        });

                    ui.separator();

                    // Menus
                    egui::MenuBar::new().ui(ui, |ui| {
                        ui.menu_button(RichText::new("File").size(12.0).color(TEXT_SECONDARY), |ui| {
                            if ui.button("New Virtual Machine…  (Ctrl+N)").clicked() {
                                self.create_form = CreateForm::default();
                                self.show_create_modal = true;
                                ui.close();
                            }
                            let has_vm = self.active_vm().is_some();
                            if ui.add_enabled(has_vm, egui::Button::new("Open VM Folder")).clicked() {
                                if let Some(vm) = self.active_vm() {
                                    self.manager.open_vm_folder(&vm.config.id);
                                }
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Exit Box!").clicked() {
                                ctx.send_viewport_cmd(ViewportCommand::Close);
                            }
                        });

                        ui.menu_button(RichText::new("View").size(12.0).color(TEXT_SECONDARY), |ui| {
                            let lbl = if self.show_sidebar {
                                "Hide Library Sidebar"
                            } else {
                                "Show Library Sidebar"
                            };
                            if ui.button(lbl).clicked() {
                                self.show_sidebar = !self.show_sidebar;
                                ui.close();
                            }
                            if ui.button("Home Tab").clicked() {
                                self.active_tab = "home".into();
                                ui.close();
                            }
                        });

                        ui.menu_button(RichText::new("VM").size(12.0).color(TEXT_SECONDARY), |ui| {
                            let vm = self.active_vm();
                            let can_start = vm.as_ref().is_some_and(|v| v.status == VmStatus::Stopped);
                            let is_running = vm.as_ref().is_some_and(|v| v.status != VmStatus::Stopped);

                            if ui.add_enabled(can_start, egui::Button::new("Power On")).clicked() {
                                if let Some(v) = &vm {
                                    self.start_vm(&v.config.id, ctx);
                                }
                                ui.close();
                            }
                            if ui.add_enabled(is_running, egui::Button::new("Power Off")).clicked() {
                                if let Some(v) = &vm {
                                    self.stop_vm(&v.config.id, false, ctx);
                                }
                                ui.close();
                            }
                            if ui.add_enabled(is_running, egui::Button::new("Pause / Resume")).clicked() {
                                if let Some(v) = &vm {
                                    let _ = self.manager.toggle_pause(&v.config.id);
                                    self.refresh();
                                }
                                ui.close();
                            }
                            if ui.add_enabled(is_running, egui::Button::new("Send Shift+F10 (CMD)")).clicked() {
                                if let Some(v) = &vm {
                                    let _ = self.manager.send_shift_f10(&v.config.id);
                                }
                                ui.close();
                            }
                            if ui.add_enabled(is_running, egui::Button::new("Send Ctrl+Alt+Del")).clicked() {
                                if let Some(v) = &vm {
                                    let _ = self.manager.send_ctrl_alt_del(&v.config.id);
                                }
                                ui.close();
                            }
                            if ui.add_enabled(is_running, egui::Button::new("Fullscreen  (Ctrl+Alt+Enter)")).clicked() {
                                self.set_fullscreen(!self.fullscreen_console, ctx);
                                ui.close();
                            }
                            ui.separator();
                            if ui.add_enabled(vm.is_some(), egui::Button::new("Snapshots…")).clicked() {
                                if let Some(v) = &vm {
                                    self.open_snapshots(&v.config.id);
                                }
                                ui.close();
                            }
                            if ui.add_enabled(vm.is_some(), egui::Button::new("Hardware Settings…")).clicked() {
                                if let Some(v) = &vm {
                                    self.open_settings(v.config.clone(), HardwareTab::Memory);
                                }
                                ui.close();
                            }
                        });
                    });

                    // Right-to-left: Window controls first, then drag fill
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        if theme::paint_window_button(ui, 2, is_maximized).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                        if theme::paint_window_button(ui, 1, is_maximized).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Maximized(!is_maximized));
                        }
                        if theme::paint_window_button(ui, 0, is_maximized).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                        }

                        // Remaining drag region
                        let drag_w = ui.available_width().max(10.0);
                        let (drag_rect, drag_resp) =
                            ui.allocate_exact_size(Vec2::new(drag_w, 34.0), Sense::click_and_drag());
                        if drag_w > 120.0 {
                            ui.painter().text(
                                drag_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                &active_title,
                                egui::FontId::proportional(11.5),
                                TEXT_MUTED,
                            );
                        }
                        if drag_resp.drag_started_by(egui::PointerButton::Primary) {
                            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                        }
                        if drag_resp.double_clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Maximized(!is_maximized));
                        }
                    });
                });
            });
    }

    fn ui_toolbar(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let active_vm = self.active_vm();
        let can_start = active_vm.as_ref().is_some_and(|v| v.status == VmStatus::Stopped);
        let is_running = active_vm.as_ref().is_some_and(|v| v.status != VmStatus::Stopped);
        let is_paused = active_vm.as_ref().is_some_and(|v| v.status == VmStatus::Paused);

        egui::Panel::top("toolbar")
            .exact_size(38.0)
            .frame(
                Frame::NONE
                    .fill(BG_TOOLBAR)
                    .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                    .inner_margin(Margin::symmetric(10, 5)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;

                    if theme::toolbar_icon_button(
                        ui,
                        IconKind::Sidebar,
                        None,
                        true,
                        self.show_sidebar,
                        if self.show_sidebar { ACCENT } else { TEXT_SECONDARY },
                    )
                    .on_hover_text("Toggle Library Sidebar")
                    .clicked()
                    {
                        self.show_sidebar = !self.show_sidebar;
                    }

                    if theme::toolbar_icon_button(
                        ui,
                        IconKind::Plus,
                        Some("New VM"),
                        true,
                        false,
                        ACCENT,
                    )
                    .on_hover_text("Create a New Virtual Machine (Ctrl+N)")
                    .clicked()
                    {
                        self.create_form = CreateForm::default();
                        self.show_create_modal = true;
                    }

                    ui.separator();

                    if theme::toolbar_icon_button(
                        ui,
                        IconKind::Play,
                        Some("Start"),
                        can_start,
                        false,
                        GREEN,
                    )
                    .on_hover_text("Power On Virtual Machine")
                    .clicked()
                    {
                        if let Some(vm) = &active_vm {
                            self.start_vm(&vm.config.id, ctx);
                        }
                    }

                    if theme::toolbar_icon_button(
                        ui,
                        IconKind::Stop,
                        None,
                        is_running,
                        false,
                        RED,
                    )
                    .on_hover_text("Power Off Virtual Machine")
                    .clicked()
                    {
                        if let Some(vm) = &active_vm {
                            self.stop_vm(&vm.config.id, false, ctx);
                        }
                    }

                    if theme::toolbar_icon_button(
                        ui,
                        if is_paused { IconKind::Play } else { IconKind::Pause },
                        None,
                        is_running,
                        false,
                        AMBER,
                    )
                    .on_hover_text(if is_paused { "Resume" } else { "Pause" })
                    .clicked()
                    {
                        if let Some(vm) = &active_vm {
                            if let Err(e) = self.manager.toggle_pause(&vm.config.id) {
                                self.error_banner = Some(e);
                            }
                            self.refresh();
                        }
                    }

                    if theme::toolbar_icon_button(
                        ui,
                        IconKind::Reset,
                        None,
                        is_running,
                        false,
                        TEXT_SECONDARY,
                    )
                    .on_hover_text("Hard Reset Virtual Machine")
                    .clicked()
                    {
                        if let Some(vm) = &active_vm {
                            match self.manager.reset_vm(&vm.config.id) {
                                Ok(Some(new_port)) => {
                                    self.vnc_clients.insert(
                                        vm.config.id.clone(),
                                        VncClient::connect(new_port, ctx.clone()),
                                    );
                                    self.refresh();
                                }
                                Ok(None) => {}
                                Err(e) => self.error_banner = Some(e),
                            }
                        }
                    }

                    ui.separator();

                    if theme::toolbar_icon_button(
                        ui,
                        IconKind::Snapshot,
                        Some("Snapshots"),
                        active_vm.is_some(),
                        false,
                        TEXT_SECONDARY,
                    )
                    .clicked()
                    {
                        if let Some(vm) = &active_vm {
                            self.open_snapshots(&vm.config.id);
                        }
                    }

                    if theme::toolbar_icon_button(
                        ui,
                        IconKind::Hardware,
                        Some("Hardware"),
                        active_vm.is_some(),
                        false,
                        TEXT_SECONDARY,
                    )
                    .clicked()
                    {
                        if let Some(vm) = &active_vm {
                            self.open_settings(vm.config.clone(), HardwareTab::Memory);
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        Frame::NONE
                            .fill(BG_INPUT)
                            .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(Margin::symmetric(10, 3))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "CPU {:>2.0}%  •  RAM {:.1}/{:.0} GB",
                                        self.host_stats.cpu_usage,
                                        self.host_stats.ram_used_gb,
                                        self.host_stats.ram_total_gb
                                    ))
                                    .size(11.0)
                                    .monospace()
                                    .color(TEXT_SECONDARY),
                                );
                            });
                    });
                });
            });
    }

    fn ui_statusbar(&self, ui: &mut egui::Ui) {
        let active_vm = self.active_vm();
        egui::Panel::bottom("statusbar")
            .exact_size(24.0)
            .frame(
                Frame::NONE
                    .fill(BG_CHROME)
                    .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                    .inner_margin(Margin::symmetric(10, 3)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    theme::paint_status_dot(
                        ui,
                        if self.engine.qemu_found { GREEN } else { RED },
                        3.5,
                    );
                    ui.label(
                        RichText::new(if self.engine.qemu_found {
                            "QEMU + WHPX Ready"
                        } else {
                            "QEMU Missing"
                        })
                        .size(11.0)
                        .color(TEXT_MUTED),
                    );

                    if let Some(vm) = &active_vm {
                        ui.separator();
                        ui.label(
                            RichText::new(format!(
                                "{} • {} • {}",
                                vm.config.name,
                                vm.config.machine_type.to_uppercase(),
                                vm.config.firmware.to_uppercase()
                            ))
                            .size(11.0)
                            .color(TEXT_SECONDARY),
                        );
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new("Box! v1.0 • Native Rust")
                                .size(11.0)
                                .color(TEXT_MUTED),
                        );
                        if let Some(vm) = &active_vm {
                            ui.separator();
                            ui.label(
                                RichText::new(format!(
                                    "{}G {} • {} • {} • {}",
                                    vm.config.disk_gb,
                                    vm.config.disk_bus.to_uppercase(),
                                    vm.config.usb_controller.to_uppercase(),
                                    vm.config.audio.to_uppercase(),
                                    if vm.config.network {
                                        vm.config.net_model.as_str()
                                    } else {
                                        "Net Off"
                                    }
                                ))
                                .size(11.0)
                                .color(TEXT_MUTED),
                            );
                        }
                    });
                });
            });
    }

    fn ui_sidebar(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        if !self.show_sidebar {
            return;
        }
        let mut clicked_vm: Option<String> = None;
        let mut dbl_clicked_vm: Option<String> = None;
        let mut ctx_action: Option<(&'static str, String)> = None;

        egui::Panel::left("library_sidebar")
            .resizable(true)
            .default_size(220.0)
            .size_range(175.0..=320.0)
            .frame(
                Frame::NONE
                    .fill(BG_SIDEBAR)
                    .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                    .inner_margin(Margin::same(10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("LIBRARY").size(10.5).strong().color(TEXT_MUTED));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("+").size(12.0)).small())
                            .on_hover_text("Create New VM")
                            .clicked()
                        {
                            self.create_form = CreateForm::default();
                            self.show_create_modal = true;
                        }
                    });
                });

                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text("Filter VMs...")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);

                let hdr_resp = ui
                    .horizontal(|ui| {
                        let (chev_rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                        theme::paint_icon(
                            ui.painter(),
                            chev_rect.center(),
                            10.0,
                            if self.library_expanded {
                                IconKind::ChevronDown
                            } else {
                                IconKind::ChevronRight
                            },
                            TEXT_MUTED,
                        );
                        ui.label(
                            RichText::new("My Computer")
                                .size(12.0)
                                .strong()
                                .color(TEXT_SECONDARY),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{}", self.vms.len()))
                                    .size(11.0)
                                    .color(TEXT_MUTED),
                            );
                        });
                    })
                    .response
                    .interact(Sense::click());
                if hdr_resp.clicked() {
                    self.library_expanded = !self.library_expanded;
                }

                ui.add_space(4.0);

                if self.library_expanded {
                    let q = self.search_query.trim().to_lowercase();
                    let filtered: Vec<VmInfo> = self
                        .vms
                        .iter()
                        .filter(|v| {
                            q.is_empty()
                                || v.config.name.to_lowercase().contains(&q)
                                || theme::os_meta(&v.config.os_type).0.to_lowercase().contains(&q)
                        })
                        .cloned()
                        .collect();

                    if filtered.is_empty() {
                        ui.add_space(16.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("No virtual machines yet.\nClick + New VM to start.")
                                    .size(11.5)
                                    .color(TEXT_MUTED),
                            );
                        });
                    } else {
                        ScrollArea::vertical().show(ui, |ui| {
                            for vm in filtered {
                                let selected = self.active_tab == vm.config.id;
                                let bg = if selected {
                                    ACCENT_BG
                                } else {
                                    Color32::TRANSPARENT
                                };
                                let border = if selected {
                                    ACCENT_BORDER
                                } else {
                                    Color32::TRANSPARENT
                                };

                                let resp = Frame::NONE
                                    .fill(bg)
                                    .stroke(Stroke::new(1.0, border))
                                    .corner_radius(CornerRadius::same(6))
                                    .inner_margin(Margin::symmetric(7, 5))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            theme::paint_os_badge(ui, &vm.config.os_type, 24.0);
                                            ui.vertical(|ui| {
                                                ui.spacing_mut().item_spacing.y = 1.0;
                                                ui.label(
                                                    RichText::new(&vm.config.name)
                                                        .size(12.0)
                                                        .strong()
                                                        .color(TEXT_PRIMARY),
                                                );
                                                ui.horizontal(|ui| {
                                                    let dot_col = match vm.status {
                                                        VmStatus::Running => GREEN,
                                                        VmStatus::Paused => AMBER,
                                                        VmStatus::Stopped => Color32::from_rgb(82, 92, 112),
                                                    };
                                                    theme::paint_status_dot(ui, dot_col, 3.0);
                                                    let sub = match vm.status {
                                                        VmStatus::Running => "Running".to_string(),
                                                        VmStatus::Paused => "Paused".to_string(),
                                                        VmStatus::Stopped => format!(
                                                            "{:.1}G • {}c",
                                                            vm.config.ram_mb as f32 / 1024.0,
                                                            vm.config.cpus
                                                        ),
                                                    };
                                                    ui.label(RichText::new(sub).size(10.5).color(TEXT_MUTED));
                                                });
                                            });
                                        });
                                    })
                                    .response
                                    .interact(Sense::click());

                                if resp.clicked() {
                                    clicked_vm = Some(vm.config.id.clone());
                                }
                                if resp.double_clicked() {
                                    dbl_clicked_vm = Some(vm.config.id.clone());
                                }
                                resp.context_menu(|ui| {
                                    if vm.status == VmStatus::Stopped {
                                        if ui.button("Power On").clicked() {
                                            ctx_action = Some(("start", vm.config.id.clone()));
                                            ui.close();
                                        }
                                    } else if ui.button("Power Off").clicked() {
                                        ctx_action = Some(("stop", vm.config.id.clone()));
                                        ui.close();
                                    }
                                    if ui.button("Hardware Settings…").clicked() {
                                        ctx_action = Some(("settings", vm.config.id.clone()));
                                        ui.close();
                                    }
                                    if ui.button("Snapshots…").clicked() {
                                        ctx_action = Some(("snapshots", vm.config.id.clone()));
                                        ui.close();
                                    }
                                    if ui.button("Open Folder").clicked() {
                                        ctx_action = Some(("folder", vm.config.id.clone()));
                                        ui.close();
                                    }
                                    ui.separator();
                                    if ui.button(RichText::new("Delete VM").color(RED)).clicked() {
                                        ctx_action = Some(("delete", vm.config.id.clone()));
                                        ui.close();
                                    }
                                });
                            }
                        });
                    }
                }
            });

        if let Some(id) = clicked_vm {
            self.open_vm_tab(&id);
        }
        if let Some(id) = dbl_clicked_vm {
            self.open_vm_tab(&id);
            if self
                .vms
                .iter()
                .find(|v| v.config.id == id)
                .is_some_and(|v| v.status == VmStatus::Stopped)
            {
                self.start_vm(&id, ctx);
            }
        }
        if let Some((act, id)) = ctx_action {
            match act {
                "start" => self.start_vm(&id, ctx),
                "stop" => self.stop_vm(&id, false, ctx),
                "settings" => {
                    if let Some(v) = self.vms.iter().find(|v| v.config.id == id) {
                        self.open_settings(v.config.clone(), HardwareTab::Memory);
                    }
                }
                "snapshots" => self.open_snapshots(&id),
                "folder" => self.manager.open_vm_folder(&id),
                "delete" => {
                    let _ = self.manager.delete_vm(&id);
                    self.close_vm_tab(&id);
                    self.refresh();
                }
                _ => {}
            }
        }
    }

    fn ui_tab_bar(&mut self, ui: &mut egui::Ui) {
        let mut to_close: Option<String> = None;
        Frame::NONE
            .fill(BG_CHROME)
            .inner_margin(Margin {
                left: 8,
                right: 8,
                top: 4,
                bottom: 0,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;

                    let home_active = self.active_tab == "home";
                    let home_resp = Frame::NONE
                        .fill(if home_active { BG_APP } else { Color32::TRANSPARENT })
                        .stroke(Stroke::new(
                            1.0,
                            if home_active { BORDER_SUBTLE } else { Color32::TRANSPARENT },
                        ))
                        .corner_radius(CornerRadius {
                            nw: 5,
                            ne: 5,
                            sw: 0,
                            se: 0,
                        })
                        .inner_margin(Margin::symmetric(12, 5))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("Home")
                                    .size(12.0)
                                    .strong()
                                    .color(if home_active { TEXT_PRIMARY } else { TEXT_SECONDARY }),
                            );
                        })
                        .response
                        .interact(Sense::click());
                    if home_resp.clicked() {
                        self.active_tab = "home".into();
                    }

                    let tabs = self.open_tabs.clone();
                    for id in tabs {
                        let Some(vm) = self.vms.iter().find(|v| v.config.id == id) else {
                            continue;
                        };
                        let active = self.active_tab == id;
                        let resp = Frame::NONE
                            .fill(if active { BG_APP } else { Color32::TRANSPARENT })
                            .stroke(Stroke::new(
                                1.0,
                                if active { BORDER_SUBTLE } else { Color32::TRANSPARENT },
                            ))
                            .corner_radius(CornerRadius {
                                nw: 5,
                                ne: 5,
                                sw: 0,
                                se: 0,
                            })
                            .inner_margin(Margin::symmetric(10, 5))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    let dot_col = match vm.status {
                                        VmStatus::Running => GREEN,
                                        VmStatus::Paused => AMBER,
                                        VmStatus::Stopped => Color32::from_rgb(82, 92, 112),
                                    };
                                    theme::paint_status_dot(ui, dot_col, 3.0);
                                    ui.label(
                                        RichText::new(&vm.config.name)
                                            .size(12.0)
                                            .color(if active { TEXT_PRIMARY } else { TEXT_SECONDARY }),
                                    );
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new("×").size(12.0).color(TEXT_MUTED),
                                            )
                                            .frame(false),
                                        )
                                        .clicked()
                                    {
                                        to_close = Some(id.clone());
                                    }
                                });
                            })
                            .response
                            .interact(Sense::click());
                        if resp.clicked() {
                            self.active_tab = id;
                        }
                    }
                });
            });

        if let Some(id) = to_close {
            self.close_vm_tab(&id);
        }

        let r = ui.available_rect_before_wrap();
        ui.painter().line_segment(
            [Pos2::new(r.left(), r.top()), Pos2::new(r.right(), r.top())],
            Stroke::new(1.0, BORDER_SUBTLE),
        );
    }

    fn ui_home_view(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical().show(ui, |ui| {
            Frame::NONE.inner_margin(Margin::symmetric(24, 22)).show(ui, |ui| {
                // Clean, calm VMware-style section header (no redundant SaaS hero banner)
                ui.label(
                    RichText::new("Home")
                        .size(19.0)
                        .strong()
                        .color(TEXT_PRIMARY),
                );
                ui.add_space(14.0);

                // 2 Spacious Quick-Start Workstation Tiles (real vector icons, zero clutter)
                let mut open_first_vm: Option<String> = None;
                let first_vm = self.vms.first().cloned();

                ui.columns(2, |cols| {
                    if self.ui_action_tile(
                        &mut cols[0],
                        IconKind::Plus,
                        ACCENT,
                        ACCENT_BG,
                        "Create a New Virtual Machine",
                        "Configure memory, vCPUs, QCOW2 disk, Q35/UEFI firmware, and mount an ISO.",
                    ) {
                        self.create_form = CreateForm::default();
                        self.show_create_modal = true;
                    }

                    let (icon2, col2, bg2, t2, d2) = if let Some(ref f) = first_vm {
                        (
                            IconKind::Play,
                            GREEN,
                            GREEN_BG,
                            format!("Open {}", f.config.name),
                            format!(
                                "{} • {:.1} GB RAM • {} vCPU • {}",
                                theme::os_meta(&f.config.os_type).0,
                                f.config.ram_mb as f32 / 1024.0,
                                f.config.cpus,
                                f.config.machine_type.to_uppercase()
                            ),
                        )
                    } else {
                        (
                            IconKind::Disc,
                            AMBER,
                            AMBER_BG,
                            "Boot a Live ISO".to_string(),
                            "Spin up a clean guest and boot any Linux or Windows ISO inside a native GPU tab."
                                .to_string(),
                        )
                    };
                    if self.ui_action_tile(&mut cols[1], icon2, col2, bg2, &t2, &d2) {
                        if let Some(ref f) = first_vm {
                            open_first_vm = Some(f.config.id.clone());
                        } else {
                            self.create_form = CreateForm::default();
                            self.show_create_modal = true;
                        }
                    }
                });

                if let Some(id) = open_first_vm {
                    self.open_vm_tab(&id);
                }

                ui.add_space(16.0);

                // Equal-height 2-column System & Virtualization Hardware cards
                ui.columns(2, |cols| {
                    Frame::NONE
                        .fill(BG_CARD)
                        .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::same(16))
                        .show(&mut cols[0], |ui| {
                            ui.set_width(ui.available_width());
                            ui.set_min_height(148.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("VIRTUALIZATION HARDWARE")
                                        .size(10.5)
                                        .strong()
                                        .color(TEXT_MUTED),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    theme::paint_status_dot(
                                        ui,
                                        if self.engine.qemu_found { GREEN } else { RED },
                                        3.5,
                                    );
                                });
                            });
                            ui.add_space(10.0);

                            egui::Grid::new("virt_hw_grid")
                                .num_columns(2)
                                .spacing([14.0, 8.0])
                                .show(ui, |ui| {
                                    ui.label(RichText::new("Host CPU").size(12.0).color(TEXT_SECONDARY));
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&self.engine.cpu_brand)
                                                .size(12.0)
                                                .color(TEXT_PRIMARY),
                                        )
                                        .truncate(),
                                    );
                                    ui.end_row();

                                    ui.label(RichText::new("Hypervisor").size(12.0).color(TEXT_SECONDARY));
                                    ui.label(
                                        RichText::new(&self.engine.qemu_version)
                                            .size(12.0)
                                            .color(TEXT_PRIMARY),
                                    );
                                    ui.end_row();

                                    ui.label(RichText::new("Acceleration").size(12.0).color(TEXT_SECONDARY));
                                    ui.label(
                                        RichText::new("WHPX (VT-x / AMD-V) + TCG")
                                            .size(12.0)
                                            .color(TEXT_PRIMARY),
                                    );
                                    ui.end_row();

                                    ui.label(RichText::new("Chipset").size(12.0).color(TEXT_SECONDARY));
                                    ui.label(
                                        RichText::new(if self.engine.ovmf_found {
                                            "Q35 PCIe • SeaBIOS / OVMF UEFI"
                                        } else {
                                            "Q35 PCIe • SeaBIOS"
                                        })
                                        .size(12.0)
                                        .color(TEXT_PRIMARY),
                                    );
                                    ui.end_row();

                                    ui.label(RichText::new("Devices").size(12.0).color(TEXT_SECONDARY));
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new("VirtIO / SATA • xHCI USB 3 • HDA • e1000")
                                                .size(12.0)
                                                .color(TEXT_PRIMARY),
                                        )
                                        .truncate(),
                                    );
                                    ui.end_row();
                                });
                        });

                    Frame::NONE
                        .fill(BG_CARD)
                        .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::same(16))
                        .show(&mut cols[1], |ui| {
                            ui.set_width(ui.available_width());
                            ui.set_min_height(148.0);
                            ui.label(
                                RichText::new("HOST RESOURCES")
                                    .size(10.5)
                                    .strong()
                                    .color(TEXT_MUTED),
                            );
                            ui.add_space(12.0);

                            self.ui_metric_bar(
                                ui,
                                &format!("CPU ({} logical cores)", self.host_stats.cpu_cores),
                                &format!("{:.1}%", self.host_stats.cpu_usage),
                                (self.host_stats.cpu_usage / 100.0).clamp(0.0, 1.0),
                                ACCENT,
                            );
                            ui.add_space(14.0);
                            let ram_frac = if self.host_stats.ram_total_gb > 0.0 {
                                (self.host_stats.ram_used_gb / self.host_stats.ram_total_gb) as f32
                            } else {
                                0.0
                            };
                            self.ui_metric_bar(
                                ui,
                                "Memory",
                                &format!(
                                    "{:.1} / {:.1} GB",
                                    self.host_stats.ram_used_gb, self.host_stats.ram_total_gb
                                ),
                                ram_frac.clamp(0.0, 1.0),
                                GREEN,
                            );
                            ui.add_space(14.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("Storage: {}", self.engine.vms_dir))
                                        .monospace()
                                        .size(11.0)
                                        .color(TEXT_MUTED),
                                )
                                .truncate(),
                            );
                        });
                });
            });
        });
    }

    fn ui_action_tile(
        &self,
        ui: &mut egui::Ui,
        icon: IconKind,
        accent: Color32,
        icon_bg: Color32,
        title: &str,
        desc: &str,
    ) -> bool {
        let resp = Frame::NONE
            .fill(BG_CARD)
            .stroke(Stroke::new(1.0, BORDER_SUBTLE))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(92.0);

                ui.horizontal(|ui| {
                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
                    ui.painter()
                        .rect_filled(icon_rect, CornerRadius::same(7), icon_bg);
                    theme::paint_icon(ui.painter(), icon_rect.center(), 15.0, icon, accent);

                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(title).size(13.5).strong().color(TEXT_PRIMARY));
                        ui.add_space(3.0);
                        ui.label(RichText::new(desc).size(12.0).color(TEXT_SECONDARY));
                    });
                });
            })
            .response
            .interact(Sense::click());

        if resp.hovered() {
            ui.painter().rect_stroke(
                resp.rect,
                CornerRadius::same(8),
                Stroke::new(1.0, ACCENT),
                StrokeKind::Inside,
            );
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        resp.clicked()
    }

    fn ui_metric_bar(&self, ui: &mut egui::Ui, label: &str, value: &str, frac: f32, color: Color32) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(11.5).color(TEXT_SECONDARY));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(value).size(11.5).monospace().color(TEXT_PRIMARY));
            });
        });
        ui.add_space(4.0);
        let (bar_rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
        ui.painter()
            .rect_filled(bar_rect, CornerRadius::same(3), Color32::from_rgb(226, 232, 240));
        let fill_rect = Rect::from_min_size(
            bar_rect.min,
            Vec2::new((bar_rect.width() * frac).max(2.0), bar_rect.height()),
        );
        ui.painter()
            .rect_filled(fill_rect, CornerRadius::same(3), color);
    }

    fn ui_vm_summary(&mut self, ui: &mut egui::Ui, vm: &VmInfo, ctx: &Context) {
        ScrollArea::vertical().show(ui, |ui| {
            Frame::NONE.inner_margin(Margin::symmetric(20, 18)).show(ui, |ui| {
                // Header card
                Frame::NONE
                    .fill(BG_CARD)
                    .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(18, 14))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            theme::paint_os_badge(ui, &vm.config.os_type, 40.0);
                            ui.add_space(6.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(&vm.config.name)
                                        .size(16.5)
                                        .strong()
                                        .color(TEXT_PRIMARY),
                                );
                                ui.horizontal(|ui| {
                                    theme::paint_status_dot(ui, Color32::from_rgb(90, 100, 120), 3.5);
                                    ui.label(
                                        RichText::new(format!(
                                            "Powered Off  •  {}  •  {} {}",
                                            theme::os_meta(&vm.config.os_type).0,
                                            vm.config.machine_type.to_uppercase(),
                                            vm.config.firmware.to_uppercase()
                                        ))
                                        .size(11.5)
                                        .color(TEXT_SECONDARY),
                                    );
                                });
                            });

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let play = egui::Button::new(
                                    RichText::new("Power On")
                                        .size(12.5)
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(GREEN_BTN)
                                .stroke(Stroke::NONE)
                                .corner_radius(CornerRadius::same(6))
                                .min_size(Vec2::new(96.0, 32.0));
                                if ui.add(play).clicked() {
                                    self.start_vm(&vm.config.id, ctx);
                                }
                            });
                        });
                    });

                ui.add_space(14.0);

                // Two-column balanced Devices + Management cards
                let mut open_hw: Option<HardwareTab> = None;
                ui.columns(2, |cols| {
                    // Left: Clickable Hardware Devices Table
                    Frame::NONE
                        .fill(BG_CARD)
                        .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::same(16))
                        .show(&mut cols[0], |ui| {
                            ui.set_width(ui.available_width());
                            ui.set_min_height(286.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("VIRTUAL HARDWARE DEVICES")
                                        .size(10.5)
                                        .strong()
                                        .color(TEXT_MUTED),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button("Edit All").clicked() {
                                        open_hw = Some(HardwareTab::Memory);
                                    }
                                });
                            });
                            ui.add_space(8.0);

                            let rows: [(HardwareTab, &str, String); 9] = [
                                (
                                    HardwareTab::Memory,
                                    "Memory",
                                    format!("{:.2} GB ({} MB)", vm.config.ram_mb as f32 / 1024.0, vm.config.ram_mb),
                                ),
                                (
                                    HardwareTab::Processors,
                                    "Processors",
                                    format!("{} vCPU ({})", vm.config.cpus, vm.config.accel.to_uppercase()),
                                ),
                                (
                                    HardwareTab::HardDisk,
                                    "Hard Disk",
                                    format!(
                                        "{} GB QCOW2 ({}) • {:.1} MB used",
                                        vm.config.disk_gb,
                                        vm.config.disk_bus.to_uppercase(),
                                        vm.disk_used_bytes as f64 / 1024.0 / 1024.0
                                    ),
                                ),
                                (
                                    HardwareTab::CdDvd,
                                    "CD/DVD (ISO)",
                                    vm.config
                                        .iso_path
                                        .as_deref()
                                        .and_then(|p| std::path::Path::new(p).file_name())
                                        .map(|f| f.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "Not connected".into()),
                                ),
                                (
                                    HardwareTab::Network,
                                    "Network Adapter",
                                    if vm.config.network {
                                        format!("NAT ({})", vm.config.net_model)
                                    } else {
                                        "Disconnected".into()
                                    },
                                ),
                                (
                                    HardwareTab::Usb,
                                    "USB Controller",
                                    match vm.config.usb_controller.as_str() {
                                        "xhci" => "USB 3.0 (xHCI + Tablet)".into(),
                                        "ehci" => "USB 2.0 (EHCI + Tablet)".into(),
                                        _ => "Disabled".into(),
                                    },
                                ),
                                (
                                    HardwareTab::Audio,
                                    "Sound Card",
                                    match vm.config.audio.as_str() {
                                        "intel-hda" => "Intel HD Audio (hda-duplex)".into(),
                                        "ac97" => "AC'97 Audio".into(),
                                        _ => "Disabled".into(),
                                    },
                                ),
                                (
                                    HardwareTab::Display,
                                    "Display / GPU",
                                    format!(
                                        "{} VGA • {} ({} MB)",
                                        vm.config.vga.to_uppercase(),
                                        vm.config.display_res,
                                        vm.config.vram_mb
                                    ),
                                ),
                                (
                                    HardwareTab::Firmware,
                                    "Firmware & Chipset",
                                    format!(
                                        "{} • {} • Boot: {}",
                                        vm.config.machine_type.to_uppercase(),
                                        vm.config.firmware.to_uppercase(),
                                        if vm.config.boot_order == "dc" { "ISO → Disk" } else { "Disk → ISO" }
                                    ),
                                ),
                            ];

                            egui::Grid::new("vm_devices_grid")
                                .num_columns(2)
                                .spacing([16.0, 7.0])
                                .show(ui, |ui| {
                                    for (tab, label, val) in rows {
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(label).size(12.0).color(ACCENT),
                                                )
                                                .frame(false),
                                            )
                                            .on_hover_cursor(CursorIcon::PointingHand)
                                            .clicked()
                                        {
                                            open_hw = Some(tab);
                                        }
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&val).size(12.0).color(TEXT_PRIMARY),
                                            )
                                            .truncate(),
                                        );
                                        ui.end_row();
                                    }
                                });
                        });

                    // Right: Management & Storage (matched height)
                    Frame::NONE
                        .fill(BG_CARD)
                        .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::same(16))
                        .show(&mut cols[1], |ui| {
                            ui.set_width(ui.available_width());
                            ui.set_min_height(286.0);
                            ui.label(
                                RichText::new("MANAGEMENT & STORAGE")
                                    .size(10.5)
                                    .strong()
                                    .color(TEXT_MUTED),
                            );
                            ui.add_space(10.0);
                            let notes = if vm.config.notes.trim().is_empty() {
                                "Click any hardware row on the left to customize Memory, CPU, ISO, USB, Audio, GPU, or UEFI firmware."
                            } else {
                                vm.config.notes.as_str()
                            };
                            ui.label(RichText::new(notes).size(12.0).color(TEXT_SECONDARY));

                            ui.add_space(14.0);
                            ui.separator();
                            ui.add_space(10.0);

                            ui.label(RichText::new("VIRTUAL DISK FILE").size(10.0).strong().color(TEXT_MUTED));
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(&vm.config.disk_path)
                                    .monospace()
                                    .size(11.0)
                                    .color(TEXT_SECONDARY),
                            );

                            ui.add_space(16.0);
                            ui.label(RichText::new("ACTIONS").size(10.0).strong().color(TEXT_MUTED));
                            ui.add_space(6.0);
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("Hardware Settings").clicked() {
                                    open_hw = Some(HardwareTab::Memory);
                                }
                                if ui.button("Snapshots").clicked() {
                                    self.open_snapshots(&vm.config.id);
                                }
                                if ui.button("Open Folder").clicked() {
                                    self.manager.open_vm_folder(&vm.config.id);
                                }
                                let del_btn = egui::Button::new(RichText::new("Delete VM").color(RED))
                                    .fill(Color32::from_rgb(254, 226, 226))
                                    .stroke(Stroke::new(1.0, Color32::from_rgb(252, 165, 165)));
                                if ui.add(del_btn).clicked() {
                                    let _ = self.manager.delete_vm(&vm.config.id);
                                    self.close_vm_tab(&vm.config.id);
                                    self.refresh();
                                }
                            });
                        });
                });

                if let Some(tab) = open_hw {
                    self.open_settings(vm.config.clone(), tab);
                }
            });
        });
    }

    fn ui_vm_console(&mut self, ui: &mut egui::Ui, vm: &VmInfo, ctx: &Context) {
        if !self.vnc_clients.contains_key(&vm.config.id) {
            if let Some(port) = vm.vnc_port {
                self.vnc_clients
                    .insert(vm.config.id.clone(), VncClient::connect(port, ctx.clone()));
            }
        }

        let mut tex_info = (None, 800usize, 600usize, VncStatus::Connecting);
        if let Some(client) = self.vnc_clients.get_mut(&vm.config.id) {
            tex_info = client.sync_texture(ctx);
        }
        let (tex_id, fb_w, fb_h, vnc_status) = tex_info;

        if !self.fullscreen_console {
            Frame::NONE
                .fill(BG_TOOLBAR)
                .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                .inner_margin(Margin::symmetric(12, 5))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        theme::paint_status_dot(
                            ui,
                            if vm.status == VmStatus::Paused { AMBER } else { GREEN },
                            3.5,
                        );
                        ui.label(RichText::new(&vm.config.name).strong().size(12.0));
                        ui.label(
                            RichText::new(format!(
                                "{}×{} • {} • VNC :{}",
                                fb_w,
                                fb_h,
                                vm.accel_active.as_deref().unwrap_or("QEMU"),
                                vm.vnc_port.unwrap_or(0)
                            ))
                            .monospace()
                            .size(11.0)
                            .color(TEXT_MUTED),
                        );

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .button(RichText::new("Power Off").size(11.5).color(RED))
                                .clicked()
                            {
                                self.stop_vm(&vm.config.id, false, ctx);
                            }
                            if ui
                                .button(RichText::new("Fullscreen").size(11.5).color(ACCENT))
                                .on_hover_text("Enter Fullscreen Console (Ctrl+Alt+Enter)")
                                .clicked()
                            {
                                self.set_fullscreen(true, ctx);
                            }
                            if ui
                                .button(
                                    RichText::new(if self.scale_fit { "1:1 Native" } else { "Scale Fit" })
                                        .size(11.5),
                                )
                                .clicked()
                            {
                                self.scale_fit = !self.scale_fit;
                            }
                            if ui.button(RichText::new("Ctrl+Alt+Del").size(11.5)).clicked() {
                                let _ = self.manager.send_ctrl_alt_del(&vm.config.id);
                            }
                            if ui
                                .button(RichText::new("Shift+F10").size(11.5))
                                .on_hover_text("Send Shift+F10 to guest (opens CMD in Windows Setup)")
                                .clicked()
                            {
                                let _ = self.manager.send_shift_f10(&vm.config.id);
                            }
                        });
                    });
                });
        }

        Frame::NONE.fill(Color32::from_rgb(8, 10, 14)).show(ui, |ui| {
            let avail = ui.available_size();
            let (stage_rect, _) = ui.allocate_exact_size(avail, Sense::hover());

            if let Some(tid) = tex_id {
                let target_size = if self.scale_fit {
                    let scale = (stage_rect.width() / fb_w as f32)
                        .min(stage_rect.height() / fb_h as f32)
                        .min(2.0);
                    Vec2::new(fb_w as f32 * scale, fb_h as f32 * scale)
                } else {
                    Vec2::new(fb_w as f32, fb_h as f32)
                };
                let img_rect = Rect::from_center_size(stage_rect.center(), target_size);
                ui.painter().image(
                    tid,
                    img_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );

                let resp = ui.interact(
                    img_rect,
                    ui.scope_id().with(&vm.config.id),
                    Sense::click_and_drag(),
                );
                if resp.clicked() {
                    resp.request_focus();
                }

                if let Some(client) = self.vnc_clients.get(&vm.config.id) {
                    if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                        if img_rect.contains(pos) {
                            let gx = (((pos.x - img_rect.left()) / img_rect.width()) * fb_w as f32)
                                .clamp(0.0, (fb_w.saturating_sub(1)) as f32) as u16;
                            let gy = (((pos.y - img_rect.top()) / img_rect.height()) * fb_h as f32)
                                .clamp(0.0, (fb_h.saturating_sub(1)) as f32) as u16;
                            let mask = ctx.input(|i| {
                                let mut m = 0u8;
                                if i.pointer.primary_down() {
                                    m |= 1;
                                }
                                if i.pointer.middle_down() {
                                    m |= 2;
                                }
                                if i.pointer.secondary_down() {
                                    m |= 4;
                                }
                                m
                            });
                            client.send_input(VncInput::Pointer { x: gx, y: gy, mask });
                        }
                    }

                    if resp.has_focus() || resp.hovered() {
                        ctx.input(|i| {
                            for ev in &i.events {
                                match ev {
                                    egui::Event::Key { key, pressed, .. } => {
                                        if let Some(sym) = egui_key_to_keysym(*key) {
                                            client.send_input(VncInput::Key {
                                                keysym: sym,
                                                down: *pressed,
                                            });
                                        }
                                    }
                                    egui::Event::Text(txt) => {
                                        for ch in txt.chars() {
                                            let code = ch as u32;
                                            let sym = if (0x20..=0x7e).contains(&code) {
                                                code
                                            } else {
                                                0x01000000 | code
                                            };
                                            client.send_input(VncInput::Key {
                                                keysym: sym,
                                                down: true,
                                            });
                                            client.send_input(VncInput::Key {
                                                keysym: sym,
                                                down: false,
                                            });
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        });
                    }
                }
            }

            if vnc_status != VncStatus::Connected {
                let msg = match vnc_status {
                    VncStatus::Connecting => format!("Connecting to {} display…", vm.config.name),
                    VncStatus::Disconnected(reason) => format!("Display disconnected: {reason}"),
                    VncStatus::Connected => String::new(),
                };
                ui.painter().text(
                    stage_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    msg,
                    egui::FontId::proportional(13.5),
                    TEXT_SECONDARY,
                );
            }
        });

        // Floating VMware-style top hover bar when in Fullscreen Console mode
        if self.fullscreen_console {
            let near_top = ctx.input(|i| {
                i.pointer
                    .hover_pos()
                    .is_some_and(|p| p.y <= 46.0)
            });
            if near_top {
                egui::Area::new(egui::Id::unique("fs_hover_bar"))
                    .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 6.0))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        Frame::NONE
                            .fill(BG_CARD)
                            .stroke(Stroke::new(1.0, BORDER_STRONG))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(12, 6))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    theme::paint_status_dot(ui, GREEN, 3.5);
                                    ui.label(RichText::new(&vm.config.name).strong().size(12.0));
                                    ui.separator();
                                    if ui.button(RichText::new("Shift+F10").size(11.5)).clicked() {
                                        let _ = self.manager.send_shift_f10(&vm.config.id);
                                    }
                                    if ui.button(RichText::new("Ctrl+Alt+Del").size(11.5)).clicked() {
                                        let _ = self.manager.send_ctrl_alt_del(&vm.config.id);
                                    }
                                    if ui
                                        .button(RichText::new("Exit Fullscreen").size(11.5).color(ACCENT))
                                        .clicked()
                                    {
                                        self.set_fullscreen(false, ctx);
                                    }
                                    if ui
                                        .button(RichText::new("Power Off").size(11.5).color(RED))
                                        .clicked()
                                    {
                                        self.stop_vm(&vm.config.id, false, ctx);
                                    }
                                });
                            });
                    });
            }
        }
    }

    fn ui_modals(&mut self, ctx: &Context) {
        // 1. New VM Wizard Modal
        if self.show_create_modal {
            let mut open = true;
            let mut cancel_clicked = false;
            egui::Window::new("New Virtual Machine Wizard")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .fixed_size(Vec2::new(500.0, 420.0))
                .show(ctx, |ui| {
                    egui::Grid::new("create_vm_grid")
                        .num_columns(2)
                        .spacing([14.0, 9.0])
                        .show(ui, |ui| {
                            ui.label("VM Name");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.create_form.name)
                                    .desired_width(320.0),
                            );
                            ui.end_row();

                            ui.label("Guest OS");
                            egui::ComboBox::from_id_salt("os_create_cb")
                                .selected_text(theme::os_meta(&self.create_form.os_type).0)
                                .width(320.0)
                                .show_ui(ui, |ui| {
                                    for (id, label, ram, cpus, disk) in [
                                        ("ubuntu", "Ubuntu Linux (64-bit)", 4096, 2, 32),
                                        ("debian", "Debian Linux (64-bit)", 2048, 2, 24),
                                        ("fedora", "Fedora Workstation (64-bit)", 4096, 2, 32),
                                        ("arch", "Arch Linux (64-bit)", 2048, 2, 24),
                                        ("windows", "Windows 10 / 11 (64-bit)", 4096, 4, 60),
                                        ("linux", "Other Linux (64-bit)", 2048, 2, 20),
                                        ("custom", "Custom OS", 1024, 1, 16),
                                    ] {
                                        if ui
                                            .selectable_value(&mut self.create_form.os_type, id.into(), label)
                                            .clicked()
                                        {
                                            self.create_form.ram_mb = ram;
                                            self.create_form.cpus = cpus;
                                            self.create_form.disk_gb = disk;
                                            self.create_form.firmware = if id == "windows" {
                                                "uefi".into()
                                            } else {
                                                "bios".into()
                                            };
                                        }
                                    }
                                });
                            ui.end_row();

                            ui.label("Installer ISO");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.create_form.iso_path)
                                        .hint_text("Optional .iso image")
                                        .desired_width(236.0),
                                );
                                if ui.button("Browse…").clicked() {
                                    if let Some(p) = rfd::FileDialog::new()
                                        .add_filter("Disk Images", &["iso", "img"])
                                        .pick_file()
                                    {
                                        self.create_form.iso_path = p.to_string_lossy().to_string();
                                    }
                                }
                            });
                            ui.end_row();

                            ui.label("Memory (MB)");
                            ui.add(
                                egui::Slider::new(&mut self.create_form.ram_mb, 512..=16384)
                                    .step_by(256.0),
                            );
                            ui.end_row();

                            ui.label("Processors");
                            ui.add(egui::Slider::new(&mut self.create_form.cpus, 1..=16));
                            ui.end_row();

                            ui.label("Disk Size (GB)");
                            ui.add(egui::Slider::new(&mut self.create_form.disk_gb, 8..=512));
                            ui.end_row();

                            ui.label("Firmware");
                            egui::ComboBox::from_id_salt("fw_create_cb")
                                .selected_text(self.create_form.firmware.to_uppercase())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.create_form.firmware, "bios".into(), "BIOS (SeaBIOS)");
                                    ui.selectable_value(&mut self.create_form.firmware, "uefi".into(), "UEFI (OVMF)");
                                });
                            ui.end_row();

                            ui.label("Acceleration");
                            egui::ComboBox::from_id_salt("accel_create_cb")
                                .selected_text(self.create_form.accel.to_uppercase())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.create_form.accel, "auto".into(), "Auto (WHPX → TCG)");
                                    ui.selectable_value(&mut self.create_form.accel, "whpx".into(), "WHPX (Hypervisor)");
                                    ui.selectable_value(&mut self.create_form.accel, "tcg".into(), "TCG (Software)");
                                });
                            ui.end_row();

                            ui.label("Peripherals");
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.create_form.network, "NAT NIC");
                                ui.label("• USB 3.0 • Intel HDA");
                            });
                            ui.end_row();
                        });

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let create_btn = egui::Button::new(
                            RichText::new("Create Virtual Machine")
                                .color(Color32::WHITE)
                                .strong(),
                        )
                        .fill(ACCENT);
                        if ui.add(create_btn).clicked() {
                            let req = CreateVmRequest {
                                name: self.create_form.name.clone(),
                                os_type: self.create_form.os_type.clone(),
                                ram_mb: self.create_form.ram_mb,
                                cpus: self.create_form.cpus,
                                disk_gb: self.create_form.disk_gb,
                                iso_path: if self.create_form.iso_path.trim().is_empty() {
                                    None
                                } else {
                                    Some(self.create_form.iso_path.trim().to_string())
                                },
                                accel: self.create_form.accel.clone(),
                                network: self.create_form.network,
                                firmware: self.create_form.firmware.clone(),
                                usb_controller: self.create_form.usb_controller.clone(),
                                audio: self.create_form.audio.clone(),
                                notes: self.create_form.notes.clone(),
                            };
                            match self.manager.create_vm(req) {
                                Ok(vm) => {
                                    self.refresh();
                                    self.open_vm_tab(&vm.config.id);
                                    cancel_clicked = true;
                                }
                                Err(e) => self.error_banner = Some(e),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            cancel_clicked = true;
                        }
                    });
                });
            if !open || cancel_clicked {
                self.show_create_modal = false;
            }
        }

        // 2. VMware-style Two-Pane Virtual Hardware Settings Modal
        if let Some(mut cfg) = self.settings_cfg.clone() {
            let mut open = true;
            let mut save_now = false;
            let mut cancel_now = false;
            egui::Window::new(format!("Virtual Machine Hardware — {}", cfg.name))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .fixed_size(Vec2::new(620.0, 400.0))
                .show(ctx, |ui| {
                    ui.horizontal_top(|ui| {
                        // Left Hardware Device List
                        Frame::NONE
                            .fill(BG_SIDEBAR)
                            .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::same(6))
                            .show(ui, |ui| {
                                ui.set_width(168.0);
                                ui.set_min_height(310.0);
                                for (tab, label) in HARDWARE_TABS {
                                    let sel = self.settings_hw_tab == tab;
                                    let resp = ui.add_sized(
                                        Vec2::new(164.0, 28.0),
                                        egui::Button::new(
                                            RichText::new(label)
                                                .size(12.0)
                                                .color(if sel { Color32::WHITE } else { TEXT_SECONDARY }),
                                        )
                                        .fill(if sel { ACCENT } else { Color32::TRANSPARENT })
                                        .stroke(Stroke::NONE),
                                    );
                                    if resp.clicked() {
                                        self.settings_hw_tab = tab;
                                    }
                                }
                            });

                        ui.add_space(12.0);

                        // Right Hardware Detail Pane
                        ui.vertical(|ui| {
                            ui.set_min_width(400.0);
                            ui.set_min_height(310.0);

                            match self.settings_hw_tab {
                                HardwareTab::Memory => {
                                    ui.label(RichText::new("Memory").size(15.0).strong());
                                    ui.label(
                                        RichText::new("Allocate RAM for this virtual machine.")
                                            .size(12.0)
                                            .color(TEXT_SECONDARY),
                                    );
                                    ui.add_space(14.0);
                                    ui.add(
                                        egui::Slider::new(&mut cfg.ram_mb, 512..=16384)
                                            .step_by(256.0)
                                            .text("MB"),
                                    );
                                    ui.add_space(8.0);
                                    ui.label(
                                        RichText::new(format!("Selected: {:.2} GB", cfg.ram_mb as f32 / 1024.0))
                                            .monospace()
                                            .color(ACCENT),
                                    );
                                }
                                HardwareTab::Processors => {
                                    ui.label(RichText::new("Processors & Virtualization Engine").size(15.0).strong());
                                    ui.add_space(12.0);
                                    ui.add(egui::Slider::new(&mut cfg.cpus, 1..=16).text("vCPU Cores"));
                                    ui.add_space(12.0);
                                    ui.label("Hypervisor Acceleration");
                                    egui::ComboBox::from_id_salt("hw_accel_cb")
                                        .selected_text(cfg.accel.to_uppercase())
                                        .width(260.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut cfg.accel, "auto".into(), "Auto (WHPX → TCG fallback)");
                                            ui.selectable_value(&mut cfg.accel, "whpx".into(), "WHPX (Windows Hypervisor Platform)");
                                            ui.selectable_value(&mut cfg.accel, "tcg".into(), "TCG (Pure Software Emulation)");
                                        });
                                }
                                HardwareTab::HardDisk => {
                                    ui.label(RichText::new("Hard Disk (QCOW2)").size(15.0).strong());
                                    ui.add_space(10.0);
                                    ui.label(format!("Provisioned Capacity: {} GB", cfg.disk_gb));
                                    ui.label(
                                        RichText::new(&cfg.disk_path)
                                            .monospace()
                                            .size(11.0)
                                            .color(TEXT_MUTED),
                                    );
                                    ui.add_space(12.0);
                                    ui.label("Storage Bus Controller");
                                    egui::ComboBox::from_id_salt("hw_bus_cb")
                                        .selected_text(cfg.disk_bus.to_uppercase())
                                        .width(260.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut cfg.disk_bus, "virtio".into(), "VirtIO Block (Fastest for Linux)");
                                            ui.selectable_value(&mut cfg.disk_bus, "sata".into(), "SATA / AHCI (Best for Windows)");
                                            ui.selectable_value(&mut cfg.disk_bus, "ide".into(), "Legacy IDE");
                                        });
                                }
                                HardwareTab::CdDvd => {
                                    ui.label(RichText::new("CD/DVD (ISO Image)").size(15.0).strong());
                                    ui.add_space(12.0);
                                    let mut iso_str = cfg.iso_path.clone().unwrap_or_default();
                                    ui.add(
                                        egui::TextEdit::singleline(&mut iso_str)
                                            .hint_text("No ISO mounted")
                                            .desired_width(360.0),
                                    );
                                    ui.add_space(8.0);
                                    ui.horizontal(|ui| {
                                        if ui.button("Browse ISO…").clicked() {
                                            if let Some(p) = rfd::FileDialog::new()
                                                .add_filter("Disk Images", &["iso", "img"])
                                                .pick_file()
                                            {
                                                iso_str = p.to_string_lossy().to_string();
                                            }
                                        }
                                        if ui.button("Eject ISO").clicked() {
                                            iso_str.clear();
                                        }
                                    });
                                    cfg.iso_path = if iso_str.trim().is_empty() {
                                        None
                                    } else {
                                        Some(iso_str.trim().to_string())
                                    };
                                }
                                HardwareTab::Network => {
                                    ui.label(RichText::new("Network Adapter").size(15.0).strong());
                                    ui.add_space(10.0);
                                    ui.checkbox(&mut cfg.network, "Connected (User-Mode NAT)");
                                    ui.add_space(10.0);
                                    ui.label("Virtual NIC Model");
                                    egui::ComboBox::from_id_salt("hw_nic_cb")
                                        .selected_text(&cfg.net_model)
                                        .width(260.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut cfg.net_model, "e1000".into(), "Intel PRO/1000 (e1000)");
                                            ui.selectable_value(&mut cfg.net_model, "virtio-net-pci".into(), "VirtIO Paravirtual NIC");
                                            ui.selectable_value(&mut cfg.net_model, "rtl8139".into(), "Realtek RTL8139");
                                        });
                                }
                                HardwareTab::Usb => {
                                    ui.label(RichText::new("USB Controller").size(15.0).strong());
                                    ui.add_space(12.0);
                                    egui::ComboBox::from_id_salt("hw_usb_cb")
                                        .selected_text(cfg.usb_controller.to_uppercase())
                                        .width(260.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut cfg.usb_controller, "xhci".into(), "USB 3.0 (qemu-xhci + Tablet)");
                                            ui.selectable_value(&mut cfg.usb_controller, "ehci".into(), "USB 2.0 (EHCI + Tablet)");
                                            ui.selectable_value(&mut cfg.usb_controller, "none".into(), "Disabled");
                                        });
                                }
                                HardwareTab::Audio => {
                                    ui.label(RichText::new("Sound Card").size(15.0).strong());
                                    ui.add_space(12.0);
                                    egui::ComboBox::from_id_salt("hw_audio_cb")
                                        .selected_text(cfg.audio.to_uppercase())
                                        .width(260.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut cfg.audio, "intel-hda".into(), "Intel HD Audio (hda-duplex)");
                                            ui.selectable_value(&mut cfg.audio, "ac97".into(), "AC'97 Audio");
                                            ui.selectable_value(&mut cfg.audio, "none".into(), "Disabled");
                                        });
                                }
                                HardwareTab::Display => {
                                    ui.label(RichText::new("Display / Graphics Adapter").size(15.0).strong());
                                    ui.add_space(12.0);
                                    ui.label("Native Display Resolution (EDID)");
                                    egui::ComboBox::from_id_salt("hw_res_cb")
                                        .selected_text(&cfg.display_res)
                                        .width(260.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut cfg.display_res, "1920x1080".into(), "1920×1080 (Full HD 16:9)");
                                            ui.selectable_value(&mut cfg.display_res, "1600x900".into(), "1600×900 (HD+ 16:9)");
                                            ui.selectable_value(&mut cfg.display_res, "1366x768".into(), "1366×768 (Laptop 16:9)");
                                            ui.selectable_value(&mut cfg.display_res, "1280x720".into(), "1280×720 (HD 16:9)");
                                            ui.selectable_value(&mut cfg.display_res, "2560x1440".into(), "2560×1440 (QHD 16:9)");
                                            ui.selectable_value(&mut cfg.display_res, "1280x800".into(), "1280×800 (16:10)");
                                        });
                                    ui.add_space(10.0);
                                    ui.label("Video Adapter");
                                    egui::ComboBox::from_id_salt("hw_vga_cb")
                                        .selected_text(cfg.vga.to_uppercase())
                                        .width(260.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut cfg.vga, "std".into(), "Standard VGA (Best Compatibility)");
                                            ui.selectable_value(&mut cfg.vga, "virtio".into(), "VirtIO GPU");
                                            ui.selectable_value(&mut cfg.vga, "qxl".into(), "QXL Paravirtual");
                                        });
                                    ui.add_space(10.0);
                                    ui.add(egui::Slider::new(&mut cfg.vram_mb, 32..=256).step_by(32.0).text("MB VRAM"));
                                }
                                HardwareTab::Firmware => {
                                    ui.label(RichText::new("Firmware, Chipset & Boot").size(15.0).strong());
                                    ui.add_space(10.0);
                                    ui.label("VM Name");
                                    ui.add(egui::TextEdit::singleline(&mut cfg.name).desired_width(260.0));
                                    ui.add_space(8.0);
                                    ui.label("Motherboard Chipset");
                                    egui::ComboBox::from_id_salt("hw_mach_cb")
                                        .selected_text(cfg.machine_type.to_uppercase())
                                        .width(260.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut cfg.machine_type, "q35".into(), "Q35 (Modern PCIe Chipset)");
                                            ui.selectable_value(&mut cfg.machine_type, "pc".into(), "i440FX (Legacy PC)");
                                        });
                                    ui.add_space(8.0);
                                    ui.label("Firmware Type");
                                    egui::ComboBox::from_id_salt("hw_fw_cb")
                                        .selected_text(cfg.firmware.to_uppercase())
                                        .width(260.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut cfg.firmware, "bios".into(), "BIOS (SeaBIOS)");
                                            ui.selectable_value(&mut cfg.firmware, "uefi".into(), "UEFI (OVMF)");
                                        });
                                    ui.add_space(8.0);
                                    ui.label("Boot Order");
                                    egui::ComboBox::from_id_salt("hw_boot_cb")
                                        .selected_text(if cfg.boot_order == "dc" { "CD/DVD → Disk" } else { "Disk → CD/DVD" })
                                        .width(260.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut cfg.boot_order, "dc".into(), "CD/DVD → Disk");
                                            ui.selectable_value(&mut cfg.boot_order, "cd".into(), "Disk → CD/DVD");
                                        });
                                }
                            }
                        });
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Save Hardware").color(Color32::WHITE).strong(),
                                )
                                .fill(ACCENT),
                            )
                            .clicked()
                        {
                            save_now = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel_now = true;
                        }
                    });
                });

            if save_now {
                if let Err(e) = self.manager.update_vm(cfg) {
                    self.error_banner = Some(e);
                }
                self.refresh();
                self.settings_cfg = None;
            } else if open && !cancel_now {
                self.settings_cfg = Some(cfg);
            } else {
                self.settings_cfg = None;
            }
        }

        // 3. Snapshot Manager Modal
        if let Some(vm_id) = self.snapshots_vm_id.clone() {
            let mut open = true;
            let mut action: Option<(&'static str, String)> = None;
            egui::Window::new("Snapshot Manager")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .fixed_size(Vec2::new(460.0, 320.0))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_snapshot_name)
                                .hint_text("Snapshot name (e.g. clean-install)")
                                .desired_width(310.0),
                        );
                        if ui
                            .add(
                                egui::Button::new(RichText::new("+ Take").color(Color32::WHITE))
                                    .fill(ACCENT),
                            )
                            .clicked()
                            && !self.new_snapshot_name.trim().is_empty()
                        {
                            action = Some(("create", self.new_snapshot_name.trim().to_string()));
                        }
                    });
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);

                    if self.snapshots_list.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(36.0);
                            ui.label(
                                RichText::new("No snapshots yet on this QCOW2 disk.")
                                    .color(TEXT_MUTED),
                            );
                        });
                    } else {
                        ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            for snap in &self.snapshots_list {
                                Frame::NONE
                                    .fill(BG_SIDEBAR)
                                    .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                                    .corner_radius(CornerRadius::same(6))
                                    .inner_margin(Margin::symmetric(10, 7))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            ui.vertical(|ui| {
                                                ui.label(RichText::new(&snap.name).strong());
                                                ui.label(
                                                    RichText::new(format!(
                                                        "#{} • {} • {}",
                                                        snap.id, snap.vm_size, snap.date
                                                    ))
                                                    .monospace()
                                                    .size(11.0)
                                                    .color(TEXT_MUTED),
                                                );
                                            });
                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                if ui
                                                    .button(RichText::new("Delete").color(RED))
                                                    .clicked()
                                                {
                                                    action = Some(("delete", snap.name.clone()));
                                                }
                                                if ui.button("Revert").clicked() {
                                                    action = Some(("restore", snap.name.clone()));
                                                }
                                            });
                                        });
                                    });
                                ui.add_space(4.0);
                            }
                        });
                    }
                });

            if let Some((act, name)) = action {
                let res = match act {
                    "create" => {
                        self.new_snapshot_name.clear();
                        self.manager.create_snapshot(&vm_id, &name)
                    }
                    "restore" => self.manager.restore_snapshot(&vm_id, &name),
                    "delete" => self.manager.delete_snapshot(&vm_id, &name),
                    _ => Ok(()),
                };
                if let Err(e) = res {
                    self.error_banner = Some(e);
                }
                if let Ok(list) = self.manager.list_snapshots(&vm_id) {
                    self.snapshots_list = list;
                }
            }
            if !open {
                self.snapshots_vm_id = None;
            }
        }
    }
}

impl eframe::App for BoxApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if self.last_poll.elapsed() >= Duration::from_millis(2500) {
            self.refresh();
            self.last_poll = Instant::now();
        }
        ctx.request_repaint_after(Duration::from_millis(2500));

        // Toggle Fullscreen Console with Ctrl+Alt+Enter
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.alt && i.key_pressed(Key::Enter)) {
            let next = !self.fullscreen_console;
            self.set_fullscreen(next, &ctx);
        }

        let in_fs_console = self.fullscreen_console
            && self.active_vm().is_some_and(|v| v.status != VmStatus::Stopped);

        if !in_fs_console {
            self.handle_frameless_resize(&ctx);

            if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::N)) {
                self.create_form = CreateForm::default();
                self.show_create_modal = true;
            }

            self.ui_titlebar(ui, &ctx);
            self.ui_toolbar(ui, &ctx);
            self.ui_statusbar(ui);
            self.ui_sidebar(ui, &ctx);
        }

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(if in_fs_console {
                Color32::from_rgb(8, 10, 14)
            } else {
                BG_APP
            }))
            .show(ui, |ui| {
                if !in_fs_console {
                    self.ui_tab_bar(ui);
                }

                if let Some(err) = self.error_banner.clone() {
                    Frame::NONE
                        .fill(Color32::from_rgb(254, 226, 226))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(248, 113, 113)))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(12, 8))
                        .outer_margin(Margin::symmetric(16, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&err)
                                        .size(12.0)
                                        .color(Color32::from_rgb(153, 27, 27)),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button("×").clicked() {
                                        self.error_banner = None;
                                    }
                                });
                            });
                        });
                }

                if let Some(vm) = self.active_vm() {
                    if vm.status != VmStatus::Stopped {
                        self.ui_vm_console(ui, &vm, &ctx);
                    } else {
                        self.ui_vm_summary(ui, &vm, &ctx);
                    }
                } else {
                    self.ui_home_view(ui);
                }
            });

        self.ui_modals(&ctx);

        if !in_fs_console {
            // Crisp 1px outer frameless window border
            let outer = ctx.viewport_rect();
            ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::unique("outer_frame_border"),
            ))
            .rect_stroke(
                outer,
                CornerRadius::ZERO,
                Stroke::new(1.0, BORDER_STRONG),
                StrokeKind::Inside,
            );
        }
    }

    fn on_exit(&mut self) {
        self.vnc_clients.clear();
        self.manager.shutdown_all();
    }
}
