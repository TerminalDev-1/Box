#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod models;
mod qemu;
mod theme;
mod vnc;

use app::BoxApp;
use egui::{IconData, ViewportBuilder};

fn generate_window_icon() -> IconData {
    let w = 64u32;
    let h = 64u32;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let cx = x as f32 - 32.0;
            let cy = y as f32 - 32.0;
            let dx = (cx.abs() - 22.0).max(0.0);
            let dy = (cy.abs() - 22.0).max(0.0);
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > 8.0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let (mut r, mut g, mut b) = (22u8, 25u8, 32u8);
            if cy <= 0.0 && cx.abs() <= 18.0 && (-cy + cx.abs() * 0.55) <= 18.0 && (-cy - cx.abs() * 0.55) >= -2.0 {
                (r, g, b) = (96, 165, 250);
            } else if cx <= 0.0 && cx >= -18.0 && cy >= cx.abs() * 0.55 - 2.0 && cy <= 18.0 + cx * 0.55 + 2.0 {
                (r, g, b) = (59, 130, 246);
            } else if cx > 0.0 && cx <= 18.0 && cy >= cx.abs() * 0.55 - 2.0 && cy <= 18.0 - cx * 0.55 + 2.0 {
                (r, g, b) = (37, 99, 235);
            }
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    IconData {
        rgba,
        width: w,
        height: h,
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Box! Workstation")
            .with_inner_size([1040.0, 660.0])
            .with_min_inner_size([760.0, 480.0])
            .with_decorations(false)
            .with_icon(generate_window_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Box! Workstation",
        options,
        Box::new(|cc| Ok(Box::new(BoxApp::new(cc)))),
    )
}
