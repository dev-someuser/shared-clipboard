// Windows system tray using tray-icon crate
// Provides Settings (opens settings window), and Quit.

#![cfg(target_os = "windows")]

use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use tray_icon::{TrayIconBuilder, menu::{MenuBuilder, MenuItem, SubmenuBuilder, MenuId, PredefinedMenuItem}, TrayIcon};

use crate::tray::Tray;

pub struct TrayController {
    connected: Arc<AtomicBool>,
    server_url: Arc<Mutex<String>>,
tray: Arc<Mutex<Option<TrayIcon>>>,
}

impl Tray for TrayController {
    fn set_connected(&self, connected: bool) {
        self.connected.store(connected, Ordering::Relaxed);
    }
}

pub fn start_tray(server_url: String, cmd_tx: tokio::sync::mpsc::UnboundedSender<crate::Command>) -> TrayController {
    let connected = Arc::new(AtomicBool::new(false));
    let server_url_arc = Arc::new(Mutex::new(server_url.clone()));

    let mut menu = MenuBuilder::new();
    // Disabled status item
    let status_id = MenuId::new("status");
    menu = menu.item("Connected • ")
               .with_id(status_id.clone())
               .enabled(false)
               .separator()
               .item("Settings")
               .separator()
               .item("Quit");

    let icon = generated_icon(true);

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu.build()))
        .with_tooltip("Shared Clipboard")
        .with_icon(icon)
        .build()
        .expect("Failed to create tray icon");

    let tray_arc = Arc::new(Mutex::new(Some(tray)));

    // Menu callbacks
    {
        use tray_icon::menu::MenuEvent;
        let server_url_for_cb = server_url_arc.clone();
        let connected_for_cb = connected.clone();
        let tray_ref = tray_arc.clone();
        let cmd_tx = cmd_tx.clone();
        std::thread::spawn(move || {
            for event in MenuEvent::receiver().iter() {
                match event.id.as_ref() {
                    "Settings" => {
                        let url = server_url_for_cb.lock().unwrap().clone();
                        let is_conn = connected_for_cb.load(Ordering::Relaxed);
                        if let Some(new_url) = crate::settings::open_settings_blocking(url, is_conn) {
                            *server_url_for_cb.lock().unwrap() = new_url.clone();
                            let _ = cmd_tx.send(crate::Command::SetUrl(new_url));
                            // Update status text
                            if let Some(tray) = tray_ref.lock().unwrap().as_ref() {
                                if let Some(menu) = tray.menu() {
                                    let _ = menu.update_item(&status_id, &format!("Connected • {}", *server_url_for_cb.lock().unwrap()));
                                }
                            }
                        }
                    }
                    "Quit" => { let _ = cmd_tx.send(crate::Command::Quit); }
                    _ => {}
                }
            }
        });
    }

    TrayController { connected, server_url: server_url_arc, tray: tray_arc }
}

fn generated_icon(connected: bool) -> tray_icon::icon::Icon {
    let size = 32;
    let s = size as usize;
    let mut rgba = vec![0u8; s*s*4];
    // Paper
    let pad = 6usize;
    for y in pad..(s-pad) {
        for x in pad..(s-pad) { let i=(y*s+x)*4; rgba[i..i+4].copy_from_slice(&[240,240,245,255]); }
    }
    // Outline
    for x in pad..(s-pad) { let i=((pad)*s+x)*4; rgba[i..i+4].copy_from_slice(&[60,60,70,255]); let j=(((s-pad-1)*s)+x)*4; rgba[j..j+4].copy_from_slice(&[60,60,70,255]); }
    for y in pad..(s-pad) { let i=((y*s)+pad)*4; rgba[i..i+4].copy_from_slice(&[60,60,70,255]); let j=((y*s)+(s-pad-1))*4; rgba[j..j+4].copy_from_slice(&[60,60,70,255]); }
    // Status dot
    let r = 4i32; let cx = s as i32 - r - 4; let cy = s as i32 - r - 4;
    let (dr,dg,db) = if connected { (46,204,113) } else { (231,76,60) };
    for dy in -r..=r { for dx in -r..=r { if dx*dx+dy*dy<=r*r { let X=(cx+dx) as usize; let Y=(cy+dy) as usize; let i=(Y*s+X)*4; rgba[i..i+4].copy_from_slice(&[dr as u8,dg as u8,db as u8,255]); } } }
    tray_icon::icon::Icon::from_rgba(rgba, size, size).expect("icon")
}

