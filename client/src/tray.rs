// Linux system tray integration using ksni (StatusNotifier)
// Provides a tray icon with a status label (disabled) and an Exit action.

#[cfg(target_os = "linux")]
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};

#[cfg(target_os = "linux")]
pub struct TrayController {
    connected: Arc<AtomicBool>,
    handle: ksni::Handle<AppTray>,
}

#[cfg(target_os = "linux")]
impl TrayController {
    pub fn set_connected(&self, connected: bool) {
        self.connected.store(connected, Ordering::Relaxed);
        self.handle.update(|t| {
            t.set_connected(connected);
        });
    }
}

#[cfg(target_os = "linux")]
pub fn start_tray(server_url: String) -> TrayController {
    let connected = Arc::new(AtomicBool::new(false));
    let tray = AppTray::new(server_url.clone(), connected.clone());
    let service = ksni::TrayService::new(tray);
    let handle = service.handle();
    // Spawn the tray service on a separate thread
    std::thread::spawn(move || {
        service.spawn();
    });

    TrayController { connected, handle }
}

#[cfg(target_os = "linux")]
struct AppTray {
    server_url: String,
    connected: Arc<AtomicBool>,
}

#[cfg(target_os = "linux")]
impl AppTray {
    fn new(server_url: String, connected: Arc<AtomicBool>) -> Self {
        Self { server_url, connected }
    }
    fn set_connected(&mut self, connected: bool) {
        self.connected.store(connected, Ordering::Relaxed);
    }
}

#[cfg(target_os = "linux")]
impl ksni::Tray for AppTray {
    fn title(&self) -> String { "Shared Clipboard".into() }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        // Generate a simple clipboard glyph with a small status dot.
        fn make_icon(size: i32, connected: bool) -> ksni::Icon {
            let s = size as usize;
            let mut data = vec![0u8; s * s * 4]; // RGBA

            fn put(data: &mut [u8], s: usize, x: usize, y: usize, r: u8, g: u8, b: u8, a: u8) {
                if x >= s || y >= s { return; }
                let i = (y * s + x) * 4;
                data[i] = r; data[i+1] = g; data[i+2] = b; data[i+3] = a;
            }
            fn fill_rect(data: &mut [u8], s: usize, x0: usize, y0: usize, x1: usize, y1: usize, r: u8, g: u8, b: u8, a: u8) {
                for y in y0..y1 { for x in x0..x1 { put(data, s, x, y, r, g, b, a); } }
            }
            fn outline(data: &mut [u8], s: usize, x0: usize, y0: usize, x1: usize, y1: usize) {
                for x in x0..x1 { put(data, s, x, y0, 60, 60, 70, 255); put(data, s, x, y1-1, 60,60,70,255); }
                for y in y0..y1 { put(data, s, x0, y, 60,60,70,255); put(data, s, x1-1, y, 60,60,70,255); }
            }

            // Clipboard body
            let pad = (size as f32 * 0.18) as usize;
            let top = pad + (size as f32 * 0.18) as usize;
            let right = s - pad;
            let bottom = s - pad;
            fill_rect(&mut data, s, pad, top, right, bottom, 240, 240, 245, 255); // paper
            // Outline
            outline(&mut data, s, pad, top, right, bottom);
            // Clip at top
            let clip_h = (size as f32 * 0.16) as usize;
            let clip_w = (size as f32 * 0.46) as usize;
            let cx0 = (s - clip_w)/2;
            let cy0 = pad;
            fill_rect(&mut data, s, cx0, cy0, cx0+clip_w, cy0+clip_h, 200, 200, 210, 255);
            outline(&mut data, s, cx0, cy0, cx0+clip_w, cy0+clip_h);

            // Status dot bottom-right
            let dot_r = (size as f32 * 0.12) as usize;
            let cx = right - dot_r - 2;
            let cy = bottom - dot_r - 2;
            let (dr,dg,db) = if connected { (46u8, 204u8, 113u8) } else { (231u8, 76u8, 60u8) };
            for y in 0..(dot_r*2) {
                for x in 0..(dot_r*2) {
                    let dx = x as i32 - dot_r as i32;
                    let dy = y as i32 - dot_r as i32;
                    if dx*dx + dy*dy <= (dot_r as i32)*(dot_r as i32) {
                        put(&mut data, s, (cx + x) as usize, (cy + y) as usize, dr, dg, db, 255);
                    }
                }
            }

            ksni::Icon { width: size, height: size, data }
        }

        let connected = self.connected.load(Ordering::Relaxed);
        vec![make_icon(16, connected), make_icon(24, connected), make_icon(32, connected)]
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let status_text = if self.connected.load(Ordering::Relaxed) {
            format!("Connected • {}", self.server_url)
        } else {
            format!("Disconnected • {}", self.server_url)
        };

        vec![
            ksni::MenuItem::Standard(ksni::menu::StandardItem {
                label: status_text,
                enabled: false,
                ..Default::default()
            }),
            ksni::MenuItem::Separator,
            ksni::MenuItem::Standard(ksni::menu::StandardItem {
                label: "Quit".into(),
                activate: Box::new(|_| {
                    // Terminate the process immediately
                    std::process::exit(0);
                }),
                ..Default::default()
            }),
        ]
    }
}

// Stubs for non-Linux targets so the code compiles conditionally
#[cfg(not(target_os = "linux"))]
pub struct TrayController;
#[cfg(not(target_os = "linux"))]
impl TrayController { pub fn set_connected(&self, _connected: bool) {} }
#[cfg(not(target_os = "linux"))]
pub fn start_tray(_server_url: String) -> TrayController { TrayController }

