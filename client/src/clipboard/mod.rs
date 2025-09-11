use crate::ClipboardData;

pub trait ClipboardBackend {
    fn get_clipboard_data(&self) -> Result<ClipboardData, Box<dyn std::error::Error + Send + Sync>>;
    fn has_content_changed(&mut self, data: &ClipboardData, from_server: bool, server_timestamp: Option<u64>) -> bool;
    fn mark_content_as_sent(&mut self, data: &ClipboardData);
    fn is_own_content_returned(&self, data: &ClipboardData) -> bool;
    fn set_clipboard_data_from_server(&mut self, data: &ClipboardData) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::LinuxClipboardManager as ClipboardManager;
#[cfg(target_os = "windows")]
pub use windows::WindowsClipboardManager as ClipboardManager;

