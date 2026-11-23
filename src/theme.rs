use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, Pos2, Rect, Stroke,
    Vec2, Visuals,
};
use std::sync::Arc;

// Crisp Native Windows 11 / VMware Workstation Light Mode Palette
pub const BG_APP: Color32 = Color32::from_rgb(243, 245, 248);
pub const BG_CHROME: Color32 = Color32::from_rgb(233, 236, 241);
pub const BG_TOOLBAR: Color32 = Color32::from_rgb(248, 249, 251);
pub const BG_SIDEBAR: Color32 = Color32::from_rgb(240, 242, 246);
pub const BG_CARD: Color32 = Color32::from_rgb(255, 255, 255);
pub const BG_CARD_HOVER: Color32 = Color32::from_rgb(234, 238, 245);
pub const BG_INPUT: Color32 = Color32::from_rgb(255, 255, 255);

pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(212, 218, 227);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(176, 185, 198);

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(17, 24, 39);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(75, 85, 99);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(107, 114, 128);

pub const ACCENT: Color32 = Color32::from_rgb(37, 99, 235);
pub const ACCENT_BG: Color32 = Color32::from_rgb(219, 234, 254);
pub const ACCENT_BORDER: Color32 = Color32::from_rgb(147, 197, 253);

pub const GREEN: Color32 = Color32::from_rgb(22, 163, 74);
pub const GREEN_BG: Color32 = Color32::from_rgb(220, 252, 231);
pub const GREEN_BTN: Color32 = Color32::from_rgb(22, 163, 74);

pub const AMBER: Color32 = Color32::from_rgb(217, 119, 6);
pub const AMBER_BG: Color32 = Color32::from_rgb(254, 243, 199);

pub const RED: Color32 = Color32::from_rgb(220, 38, 38);

pub fn install_workstation_theme(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    if let Ok(segoe) = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf") {
        fonts.font_data.insert(
            "segoe_ui".to_owned(),
            Arc::new(FontData::from_owned(segoe)),
        );
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "segoe_ui".to_owned());
    }
    if let Ok(consola) = std::fs::read("C:\\Windows\\Fonts\\consola.ttf") {
        fonts.font_data.insert(
            "consolas".to_owned(),
            Arc::new(FontData::from_owned(consola)),
        );
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "consolas".to_owned());
    }
    ctx.set_fonts(fonts);

    let mut visuals = Visuals::light();
    visuals.window_fill = BG_CARD;
    visuals.panel_fill = BG_APP;
    visuals.extreme_bg_color = BG_INPUT;
    visuals.faint_bg_color = BG_CHROME;
    visuals.window_stroke = Stroke::new(1.0, BORDER_STRONG);
    visuals.window_corner_radius = CornerRadius::same(8);

    visuals.widgets.noninteractive.bg_fill = BG_CARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(5);

    visuals.widgets.inactive.bg_fill = Color32::from_rgb(245, 247, 250);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(245, 247, 250);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(5);

    visuals.widgets.hovered.bg_fill = BG_CARD_HOVER;
    visuals.widgets.hovered.weak_bg_fill = BG_CARD_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(5);

    visuals.widgets.active.bg_fill = ACCENT_BG;
    visuals.widgets.active.weak_bg_fill = ACCENT_BG;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.corner_radius = CornerRadius::same(5);

    visuals.selection.bg_fill = ACCENT_BG;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    ctx.set_visuals_of(egui::Theme::Light, visuals);
    ctx.set_theme(egui::Theme::Light);

    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.window_margin = egui::Margin::same(16);
    ctx.set_style_of(egui::Theme::Light, style);
}

pub fn os_meta(os_type: &str) -> (&'static str, &'static str, Color32) {
    match os_type {
        "ubuntu" => ("Ubuntu Linux", "UB", Color32::from_rgb(234, 88, 12)),
        "debian" => ("Debian Linux", "DB", Color32::from_rgb(225, 29, 72)),
        "fedora" => ("Fedora", "FD", Color32::from_rgb(37, 99, 235)),
        "arch" => ("Arch Linux", "AR", Color32::from_rgb(2, 132, 199)),
        "windows" => ("Windows 10/11", "WN", Color32::from_rgb(37, 99, 235)),
        "linux" => ("Linux", "LX", Color32::from_rgb(161, 98, 7)),
        _ => ("Custom OS", "OS", Color32::from_rgb(71, 85, 105)),
    }
}

pub fn paint_box_logo(painter: &egui::Painter, center: Pos2, r: f32) {
    let top = Pos2::new(center.x, center.y - r);
    let right_top = Pos2::new(center.x + r * 0.86, center.y - r * 0.45);
    let left_top = Pos2::new(center.x - r * 0.86, center.y - r * 0.45);
    let mid = Pos2::new(center.x, center.y + r * 0.08);
    let right_bot = Pos2::new(center.x + r * 0.86, center.y + r * 0.52);
    let left_bot = Pos2::new(center.x - r * 0.86, center.y + r * 0.52);
    let bot = Pos2::new(center.x, center.y + r);

    painter.add(egui::Shape::convex_polygon(
        vec![top, right_top, mid, left_top],
        Color32::from_rgb(96, 165, 250),
        Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![left_top, mid, bot, left_bot],
        Color32::from_rgb(37, 99, 235),
        Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![mid, right_top, right_bot, bot],
        Color32::from_rgb(29, 78, 216),
        Stroke::NONE,
    ));
}

#[derive(Clone, Copy)]
pub enum IconKind {
    Sidebar,
    Plus,
    Play,
    Stop,
    Pause,
    Reset,
    Snapshot,
    Hardware,
    ChevronDown,
    ChevronRight,
    Disc,
}

pub fn paint_icon(painter: &egui::Painter, center: Pos2, size: f32, kind: IconKind, color: Color32) {
    let r = size * 0.44;
    let stroke = Stroke::new(1.5, color);
    match kind {
        IconKind::Sidebar => {
            let rect = Rect::from_center_size(center, Vec2::new(size * 0.88, size * 0.72));
            painter.rect_stroke(rect, CornerRadius::same(2), stroke, egui::StrokeKind::Inside);
            let split_x = rect.left() + rect.width() * 0.34;
            painter.line_segment(
                [Pos2::new(split_x, rect.top() + 1.0), Pos2::new(split_x, rect.bottom() - 1.0)],
                stroke,
            );
        }
        IconKind::Plus => {
            painter.line_segment(
                [Pos2::new(center.x - r, center.y), Pos2::new(center.x + r, center.y)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(center.x, center.y - r), Pos2::new(center.x, center.y + r)],
                stroke,
            );
        }
        IconKind::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(center.x - r * 0.65, center.y - r * 0.85),
                    Pos2::new(center.x + r * 0.9, center.y),
                    Pos2::new(center.x - r * 0.65, center.y + r * 0.85),
                ],
                color,
                Stroke::NONE,
            ));
        }
        IconKind::Stop => {
            let rect = Rect::from_center_size(center, Vec2::splat(r * 1.45));
            painter.rect_filled(rect, CornerRadius::same(1), color);
        }
        IconKind::Pause => {
            let w = r * 0.45;
            let h = r * 1.5;
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(center.x - r * 0.42, center.y), Vec2::new(w, h)),
                CornerRadius::same(1),
                color,
            );
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(center.x + r * 0.42, center.y), Vec2::new(w, h)),
                CornerRadius::same(1),
                color,
            );
        }
        IconKind::Reset => {
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2
                        + (i as f32 / 16.0) * std::f32::consts::TAU * 0.78;
                    Pos2::new(center.x + a.cos() * r, center.y + a.sin() * r)
                })
                .collect();
            painter.add(egui::Shape::line(pts, stroke));
            let tip = Pos2::new(center.x, center.y - r);
            painter.line_segment([tip, Pos2::new(tip.x - 3.5, tip.y - 2.0)], stroke);
            painter.line_segment([tip, Pos2::new(tip.x - 3.0, tip.y + 3.0)], stroke);
        }
        IconKind::Snapshot => {
            painter.circle_stroke(center, r, stroke);
            painter.line_segment([center, Pos2::new(center.x, center.y - r * 0.55)], stroke);
            painter.line_segment([center, Pos2::new(center.x + r * 0.45, center.y + r * 0.2)], stroke);
        }
        IconKind::Hardware => {
            let rect = Rect::from_center_size(center, Vec2::splat(r * 1.45));
            painter.rect_stroke(rect, CornerRadius::same(2), stroke, egui::StrokeKind::Inside);
            let inner = Rect::from_center_size(center, Vec2::splat(r * 0.6));
            painter.rect_filled(inner, CornerRadius::same(1), color);
        }
        IconKind::ChevronDown => {
            painter.add(egui::Shape::line(
                vec![
                    Pos2::new(center.x - r * 0.7, center.y - r * 0.35),
                    Pos2::new(center.x, center.y + r * 0.45),
                    Pos2::new(center.x + r * 0.7, center.y - r * 0.35),
                ],
                stroke,
            ));
        }
        IconKind::ChevronRight => {
            painter.add(egui::Shape::line(
                vec![
                    Pos2::new(center.x - r * 0.35, center.y - r * 0.7),
                    Pos2::new(center.x + r * 0.45, center.y),
                    Pos2::new(center.x - r * 0.35, center.y + r * 0.7),
                ],
                stroke,
            ));
        }
        IconKind::Disc => {
            painter.circle_stroke(center, r, stroke);
            painter.circle_stroke(center, r * 0.32, stroke);
        }
    }
}

pub fn toolbar_icon_button(
    ui: &mut egui::Ui,
    icon: IconKind,
    label: Option<&str>,
    enabled: bool,
    active: bool,
    icon_color: Color32,
) -> egui::Response {
    let text_w = label.map(|l| l.len() as f32 * 6.8 + 8.0).unwrap_or(0.0);
    let size = Vec2::new(28.0 + text_w, 26.0);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);

    let bg = if active {
        ACCENT_BG
    } else if enabled && resp.hovered() {
        BG_CARD_HOVER
    } else {
        Color32::TRANSPARENT
    };
    let border = if active {
        Stroke::new(1.0, ACCENT_BORDER)
    } else if enabled && resp.hovered() {
        Stroke::new(1.0, BORDER_SUBTLE)
    } else {
        Stroke::NONE
    };
    ui.painter()
        .rect(rect, CornerRadius::same(5), bg, border, egui::StrokeKind::Inside);

    let fg_icon = if enabled {
        icon_color
    } else {
        Color32::from_rgb(176, 184, 196)
    };
    let fg_text = if enabled {
        if active { ACCENT } else { TEXT_PRIMARY }
    } else {
        Color32::from_rgb(165, 174, 188)
    };

    let icon_center = if label.is_some() {
        Pos2::new(rect.left() + 14.0, rect.center().y)
    } else {
        rect.center()
    };
    paint_icon(ui.painter(), icon_center, 13.0, icon, fg_icon);

    if let Some(text) = label {
        ui.painter().text(
            Pos2::new(rect.left() + 26.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::proportional(12.0),
            fg_text,
        );
    }
    resp
}

pub fn paint_os_badge(ui: &mut egui::Ui, os_type: &str, size: f32) {
    let (_, abbr, col) = os_meta(os_type);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    // Soft pastel light-mode badge tint
    let bg = Color32::from_rgb(
        (255 - (255 - col.r() as u16) / 6) as u8,
        (255 - (255 - col.g() as u16) / 6) as u8,
        (255 - (255 - col.b() as u16) / 6) as u8,
    );
    let border = Color32::from_rgb(
        (255 - (255 - col.r() as u16) / 2) as u8,
        (255 - (255 - col.g() as u16) / 2) as u8,
        (255 - (255 - col.b() as u16) / 2) as u8,
    );
    ui.painter().rect(
        rect,
        CornerRadius::same((size * 0.22) as u8),
        bg,
        Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        abbr,
        egui::FontId::proportional((size * 0.37).max(9.5)),
        col,
    );
}

pub fn paint_status_dot(ui: &mut egui::Ui, color: Color32, radius: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(radius * 2.4), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), radius, color);
}

pub fn paint_window_button(ui: &mut egui::Ui, kind: u8, is_maximized: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(42.0, 34.0), egui::Sense::click());
    let hovered = resp.hovered();
    let bg = if hovered {
        if kind == 2 {
            Color32::from_rgb(225, 29, 72)
        } else {
            Color32::from_rgb(218, 223, 232)
        }
    } else {
        Color32::TRANSPARENT
    };
    if hovered {
        ui.painter().rect_filled(rect, CornerRadius::ZERO, bg);
    }
    let fg = if hovered && kind == 2 {
        Color32::WHITE
    } else {
        TEXT_PRIMARY
    };
    let c = rect.center();
    let p = ui.painter();
    match kind {
        0 => {
            p.line_segment(
                [Pos2::new(c.x - 5.0, c.y), Pos2::new(c.x + 5.0, c.y)],
                Stroke::new(1.2, fg),
            );
        }
        1 => {
            if is_maximized {
                let r1 = Rect::from_center_size(Pos2::new(c.x - 1.0, c.y + 1.0), Vec2::splat(8.0));
                p.rect_stroke(r1, CornerRadius::ZERO, Stroke::new(1.1, fg), egui::StrokeKind::Inside);
                p.line_segment(
                    [Pos2::new(c.x - 3.0, c.y - 4.5), Pos2::new(c.x + 5.0, c.y - 4.5)],
                    Stroke::new(1.1, fg),
                );
                p.line_segment(
                    [Pos2::new(c.x + 5.0, c.y - 4.5), Pos2::new(c.x + 5.0, c.y + 3.0)],
                    Stroke::new(1.1, fg),
                );
            } else {
                let r = Rect::from_center_size(c, Vec2::splat(9.0));
                p.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.1, fg), egui::StrokeKind::Inside);
            }
        }
        _ => {
            p.line_segment(
                [Pos2::new(c.x - 4.5, c.y - 4.5), Pos2::new(c.x + 4.5, c.y + 4.5)],
                Stroke::new(1.3, fg),
            );
            p.line_segment(
                [Pos2::new(c.x + 4.5, c.y - 4.5), Pos2::new(c.x - 4.5, c.y + 4.5)],
                Stroke::new(1.3, fg),
            );
        }
    }
    resp
}
