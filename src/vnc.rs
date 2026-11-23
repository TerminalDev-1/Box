use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum VncInput {
    Pointer { x: u16, y: u16, mask: u8 },
    Key { keysym: u32, down: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VncStatus {
    Connecting,
    Connected,
    Disconnected(String),
}

pub struct DirtyPatch {
    pub x: usize,
    pub y: usize,
    pub image: ColorImage,
}

pub struct SharedFramebuffer {
    pub width: usize,
    pub height: usize,
    pub resized: bool,
    pub full_image: Option<ColorImage>,
    pub patches: Vec<DirtyPatch>,
    pub status: VncStatus,
}

pub struct VncClient {
    pub port: u16,
    pub shared: Arc<Mutex<SharedFramebuffer>>,
    pub input_tx: Sender<VncInput>,
    pub stop_flag: Arc<AtomicBool>,
    pub texture: Option<TextureHandle>,
}

impl VncClient {
    pub fn connect(port: u16, ctx: Context) -> Self {
        let shared = Arc::new(Mutex::new(SharedFramebuffer {
            width: 800,
            height: 600,
            resized: false,
            full_image: None,
            patches: Vec::new(),
            status: VncStatus::Connecting,
        }));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let (input_tx, input_rx) = mpsc::channel::<VncInput>();

        let shared_bg = shared.clone();
        let stop_bg = stop_flag.clone();

        std::thread::spawn(move || {
            let mut stream_opt = None;
            for _ in 0..50 {
                if stop_bg.load(Ordering::Relaxed) {
                    return;
                }
                match TcpStream::connect(("127.0.0.1", port)) {
                    Ok(s) => {
                        let _ = s.set_nodelay(true);
                        stream_opt = Some(s);
                        break;
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(150)),
                }
            }

            let mut stream = match stream_opt {
                Some(s) => s,
                None => {
                    if let Ok(mut fb) = shared_bg.lock() {
                        fb.status = VncStatus::Disconnected("Timed out connecting to QEMU VNC".into());
                    }
                    ctx.request_repaint();
                    return;
                }
            };

            // Writer thread for input events so keyboard/mouse never block framebuffer reads
            if let Ok(mut write_stream) = stream.try_clone() {
                let stop_w = stop_bg.clone();
                std::thread::spawn(move || {
                    while !stop_w.load(Ordering::Relaxed) {
                        match input_rx.recv_timeout(Duration::from_millis(100)) {
                            Ok(VncInput::Pointer { x, y, mask }) => {
                                let mut msg = [0u8; 6];
                                msg[0] = 5;
                                msg[1] = mask;
                                msg[2..4].copy_from_slice(&x.to_be_bytes());
                                msg[4..6].copy_from_slice(&y.to_be_bytes());
                                if write_stream.write_all(&msg).is_err() {
                                    break;
                                }
                            }
                            Ok(VncInput::Key { keysym, down }) => {
                                let mut msg = [0u8; 8];
                                msg[0] = 4;
                                msg[1] = if down { 1 } else { 0 };
                                msg[4..8].copy_from_slice(&keysym.to_be_bytes());
                                if write_stream.write_all(&msg).is_err() {
                                    break;
                                }
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    }
                });
            }

            let res = run_rfb_loop(&mut stream, &shared_bg, &stop_bg, &ctx);
            if let Ok(mut fb) = shared_bg.lock() {
                fb.status = VncStatus::Disconnected(
                    res.err().unwrap_or_else(|| "VM display closed".to_string()),
                );
            }
            ctx.request_repaint();
        });

        Self {
            port,
            shared,
            input_tx,
            stop_flag,
            texture: None,
        }
    }

    pub fn sync_texture(&mut self, ctx: &Context) -> (Option<egui::TextureId>, usize, usize, VncStatus) {
        let mut fb = self.shared.lock().unwrap();
        let w = fb.width;
        let h = fb.height;
        let status = fb.status.clone();

        if fb.resized {
            fb.resized = false;
            if let Some(full) = fb.full_image.take() {
                self.texture = Some(ctx.load_texture(
                    format!("vnc-{}", self.port),
                    full,
                    TextureOptions::LINEAR,
                ));
            }
        }
        if !fb.patches.is_empty() {
            let patches = std::mem::take(&mut fb.patches);
            if self.texture.is_none() {
                let blank = ColorImage::filled([w.max(1), h.max(1)], egui::Color32::BLACK);
                self.texture = Some(ctx.load_texture(
                    format!("vnc-{}", self.port),
                    blank,
                    TextureOptions::LINEAR,
                ));
            }
            if let Some(ref mut tex) = self.texture {
                let [tw, th] = tex.size();
                for p in patches {
                    let pw = p.image.width();
                    let ph = p.image.height();
                    if pw > 0 && ph > 0 && p.x + pw <= tw && p.y + ph <= th {
                        tex.set_partial([p.x, p.y], p.image, TextureOptions::LINEAR);
                    }
                }
            }
        }

        (self.texture.as_ref().map(|t| t.id()), w, h, status)
    }

    pub fn send_input(&self, ev: VncInput) {
        let _ = self.input_tx.send(ev);
    }

    pub fn disconnect(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }
}

impl Drop for VncClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn send_fb_req(stream: &mut TcpStream, incremental: bool, w: u16, h: u16) -> Result<(), String> {
    let mut req = [0u8; 10];
    req[0] = 3;
    req[1] = if incremental { 1 } else { 0 };
    req[6..8].copy_from_slice(&w.to_be_bytes());
    req[8..10].copy_from_slice(&h.to_be_bytes());
    stream.write_all(&req).map_err(|e| e.to_string())
}

fn run_rfb_loop(
    stream: &mut TcpStream,
    shared: &Arc<Mutex<SharedFramebuffer>>,
    stop_flag: &Arc<AtomicBool>,
    ctx: &Context,
) -> Result<(), String> {
    // 1. Version
    let mut ver = [0u8; 12];
    stream.read_exact(&mut ver).map_err(|e| e.to_string())?;
    stream
        .write_all(b"RFB 003.008\n")
        .map_err(|e| e.to_string())?;

    // 2. Security types
    let mut sec_cnt = [0u8; 1];
    stream.read_exact(&mut sec_cnt).map_err(|e| e.to_string())?;
    let mut sec_types = vec![0u8; sec_cnt[0] as usize];
    stream
        .read_exact(&mut sec_types)
        .map_err(|e| e.to_string())?;
    stream.write_all(&[1u8]).map_err(|e| e.to_string())?; // None auth

    // 3. Security result
    let mut sec_res = [0u8; 4];
    stream.read_exact(&mut sec_res).map_err(|e| e.to_string())?;

    // 4. ClientInit (shared = 1)
    stream.write_all(&[1u8]).map_err(|e| e.to_string())?;

    // 5. ServerInit
    let mut init = [0u8; 24];
    stream.read_exact(&mut init).map_err(|e| e.to_string())?;
    let mut fb_w = u16::from_be_bytes([init[0], init[1]]);
    let mut fb_h = u16::from_be_bytes([init[2], init[3]]);
    let name_len = u32::from_be_bytes([init[20], init[21], init[22], init[23]]) as usize;
    let mut name_buf = vec![0u8; name_len];
    stream
        .read_exact(&mut name_buf)
        .map_err(|e| e.to_string())?;

    // 6. SetPixelFormat: 32bpp RGBA little-endian
    let mut pf = [0u8; 20];
    pf[0] = 0;
    pf[4] = 32;
    pf[5] = 24;
    pf[6] = 0;
    pf[7] = 1;
    pf[8..10].copy_from_slice(&255u16.to_be_bytes());
    pf[10..12].copy_from_slice(&255u16.to_be_bytes());
    pf[12..14].copy_from_slice(&255u16.to_be_bytes());
    pf[14] = 0;
    pf[15] = 8;
    pf[16] = 16;
    stream.write_all(&pf).map_err(|e| e.to_string())?;

    // 7. SetEncodings: Raw (0), DesktopSize (-223)
    let mut enc = [0u8; 12];
    enc[0] = 2;
    enc[2..4].copy_from_slice(&2u16.to_be_bytes());
    enc[4..8].copy_from_slice(&0i32.to_be_bytes());
    enc[8..12].copy_from_slice(&(-223i32).to_be_bytes());
    stream.write_all(&enc).map_err(|e| e.to_string())?;

    {
        let mut fb = shared.lock().unwrap();
        fb.width = fb_w as usize;
        fb.height = fb_h as usize;
        fb.resized = true;
        fb.full_image = Some(ColorImage::filled([fb_w as usize, fb_h as usize], egui::Color32::BLACK));
        fb.status = VncStatus::Connected;
    }
    ctx.request_repaint();

    send_fb_req(stream, false, fb_w, fb_h)?;

    while !stop_flag.load(Ordering::Relaxed) {
        let mut msg_type = [0u8; 1];
        if stream.read_exact(&mut msg_type).is_err() {
            break;
        }
        match msg_type[0] {
            0 => {
                // FramebufferUpdate
                let mut hdr = [0u8; 3];
                stream.read_exact(&mut hdr).map_err(|e| e.to_string())?;
                let rect_count = u16::from_be_bytes([hdr[1], hdr[2]]);
                let mut did_resize = false;

                for _ in 0..rect_count {
                    let mut rh = [0u8; 12];
                    stream.read_exact(&mut rh).map_err(|e| e.to_string())?;
                    let x = u16::from_be_bytes([rh[0], rh[1]]) as usize;
                    let y = u16::from_be_bytes([rh[2], rh[3]]) as usize;
                    let w = u16::from_be_bytes([rh[4], rh[5]]) as usize;
                    let h = u16::from_be_bytes([rh[6], rh[7]]) as usize;
                    let enc_type = i32::from_be_bytes([rh[8], rh[9], rh[10], rh[11]]);

                    if enc_type == -223 {
                        fb_w = w as u16;
                        fb_h = h as u16;
                        did_resize = true;
                        let mut fb = shared.lock().unwrap();
                        fb.width = w;
                        fb.height = h;
                        fb.resized = true;
                        fb.full_image = Some(ColorImage::filled([w, h], egui::Color32::BLACK));
                    } else if enc_type == 0 {
                        let byte_len = w * h * 4;
                        let mut pixels = vec![0u8; byte_len];
                        stream.read_exact(&mut pixels).map_err(|e| e.to_string())?;
                        // Force alpha = 255 (VNC RGBX trap!)
                        for chunk in pixels.chunks_exact_mut(4) {
                            chunk[3] = 255;
                        }
                        if w > 0 && h > 0 {
                            let img = ColorImage::from_rgba_unmultiplied([w, h], &pixels);
                            let mut fb = shared.lock().unwrap();
                            fb.patches.push(DirtyPatch { x, y, image: img });
                        }
                    } else {
                        return Err(format!("Unsupported VNC encoding: {enc_type}"));
                    }
                }
                ctx.request_repaint();
                send_fb_req(stream, !did_resize, fb_w, fb_h)?;
            }
            1 => {
                // SetColourMapEntries
                let mut h = [0u8; 5];
                stream.read_exact(&mut h).map_err(|e| e.to_string())?;
                let n = u16::from_be_bytes([h[3], h[4]]) as usize;
                let mut skip = vec![0u8; n * 6];
                stream.read_exact(&mut skip).map_err(|e| e.to_string())?;
            }
            2 => {
                // Bell
            }
            3 => {
                // ServerCutText
                let mut h = [0u8; 7];
                stream.read_exact(&mut h).map_err(|e| e.to_string())?;
                let len = u32::from_be_bytes([h[3], h[4], h[5], h[6]]) as usize;
                let mut skip = vec![0u8; len];
                stream.read_exact(&mut skip).map_err(|e| e.to_string())?;
            }
            _ => break,
        }
    }

    Ok(())
}

pub fn egui_key_to_keysym(key: egui::Key) -> Option<u32> {
    use egui::Key;
    Some(match key {
        Key::Backspace => 0xff08,
        Key::Tab => 0xff09,
        Key::Enter => 0xff0d,
        Key::Escape => 0xff1b,
        Key::Insert => 0xff63,
        Key::Delete => 0xffff,
        Key::Home => 0xff50,
        Key::End => 0xff57,
        Key::PageUp => 0xff55,
        Key::PageDown => 0xff56,
        Key::ArrowLeft => 0xff51,
        Key::ArrowUp => 0xff52,
        Key::ArrowRight => 0xff53,
        Key::ArrowDown => 0xff54,
        Key::F1 => 0xffbe,
        Key::F2 => 0xffbf,
        Key::F3 => 0xffc0,
        Key::F4 => 0xffc1,
        Key::F5 => 0xffc2,
        Key::F6 => 0xffc3,
        Key::F7 => 0xffc4,
        Key::F8 => 0xffc5,
        Key::F9 => 0xffc6,
        Key::F10 => 0xffc7,
        Key::F11 => 0xffc8,
        Key::F12 => 0xffc9,
        _ => return None,
    })
}
