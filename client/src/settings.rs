use eframe::egui;
use std::sync::{Arc, Mutex};

// На Linux запускаем отдельный процесс с флагом --settings и передаем URL через аргументы.
#[cfg(target_os = "linux")]
pub fn open_settings_blocking(current_url: String, connected: bool) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let status_flag = if connected { "--connected" } else { "--disconnected" };
    let output = std::process::Command::new(exe)
        .arg("--settings")
        .arg(format!("--url={}", current_url))
        .arg(status_flag)
        .output()
        .ok()?;
    if !output.status.success() { return None; }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

// Non-Linux or direct UI path: run UI in-process
#[cfg(not(target_os = "linux"))]
pub fn open_settings_blocking(current_url: String, connected: bool) -> Option<String> {
    run_settings_ui(current_url, connected)
}

pub fn run_settings_ui(current_url: String, connected: bool) -> Option<String> {
    struct App {
        url_input: String,
        connected: bool,
        test_result: Option<String>,
        saved_url: Arc<Mutex<Option<String>>>,
        did_setup: bool,
    }

    impl eframe::App for App {
        fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
            if !self.did_setup {
                // Force a sensible scale factor in case winit provides 0 or very small values
                ctx.set_pixels_per_point(1.5);

                // Enlarge default text styles to ensure readability on HiDPI/Wayland
                let mut style = (*ctx.style()).clone();
                use egui::FontId;
                style.text_styles = [
                    (egui::TextStyle::Small,   FontId::proportional(14.0)),
                    (egui::TextStyle::Body,    FontId::proportional(18.0)),
                    (egui::TextStyle::Button,  FontId::proportional(18.0)),
                    (egui::TextStyle::Heading, FontId::proportional(22.0)),
                    (egui::TextStyle::Monospace, FontId::monospace(16.0)),
                ].into();
                ctx.set_style(style);

                self.did_setup = true;
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Shared Clipboard - Settings");
                ui.separator();

                // Status
                let status = if self.connected { "Connected" } else { "Disconnected" };
                ui.label(format!("Status: {}", status));

                ui.horizontal(|ui| {
                    ui.label("Server URL:");
                    let te = egui::TextEdit::singleline(&mut self.url_input).hint_text("http://127.0.0.1:8080");
                    ui.add(te);
                });

                ui.horizontal(|ui| {
                    if ui.button("Test connection").clicked() {
                        let url = self.url_input.clone();
                        let res = test_connect(&url);
                        self.test_result = Some(res);
                    }
                    if ui.button("Save").clicked() {
                        *self.saved_url.lock().unwrap() = Some(self.url_input.clone());
                        let ctx2 = ctx.clone();
                        std::thread::spawn(move || {
                            // Defer close to avoid deadlock in the same update frame
                            std::thread::sleep(std::time::Duration::from_millis(10));
                            ctx2.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
                        });
                    }
                    if ui.button("Close").clicked() {
                        let ctx2 = ctx.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(10));
                            ctx2.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
                        });
                    }
                });

                if let Some(msg) = &self.test_result {
                    ui.label(msg);
                }
            });
        }
    }

    fn test_connect(base: &str) -> String {
        let url = format!("{}/api/clipboard", base.trim_end_matches('/'));
        let client = match reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build() {
            Ok(c) => c,
            Err(e) => return format!("Error: {}", e),
        };
        match client.get(url).send() {
            Ok(resp) => format!("HTTP {}", resp.status()),
            Err(e) => format!("Error: {}", e),
        }
    }

    let saved_url = Arc::new(Mutex::new(None));
    let app = App { url_input: current_url.clone(), connected, test_result: None, saved_url: saved_url.clone(), did_setup: false };

    // Default options; eframe/winit handle platform specifics
    let native_options = eframe::NativeOptions::default();

    let _ = eframe::run_native(
        "Settings",
        native_options,
        Box::new(|_cc| Ok(Box::new(app))),
    );

    Arc::try_unwrap(saved_url).ok().and_then(|m| m.into_inner().ok()).and_then(|v| v)
}

