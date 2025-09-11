use crate::ClipboardData;
use super::ClipboardBackend;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::debug;
use clipboard_win::{formats, get_clipboard, set_clipboard};

pub struct WindowsClipboardManager {
    last_content_hash: Option<u64>,
    last_server_timestamp: Option<u64>,
    last_sent_hash: Option<u64>,
}

impl WindowsClipboardManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self { last_content_hash: None, last_server_timestamp: None, last_sent_hash: None })
    }
    fn calculate_content_hash(data: &ClipboardData) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        data.content.hash(&mut hasher);
        if let Some(ref html) = data.html { html.hash(&mut hasher); }
        if let Some(ref rtf) = data.rtf { rtf.hash(&mut hasher); }
        hasher.finish()
    }
}

impl ClipboardBackend for WindowsClipboardManager {
    fn get_clipboard_data(&self) -> Result<ClipboardData, Box<dyn std::error::Error + Send + Sync>> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let plain_text = match get_clipboard(formats::Unicode) { Ok(t) => t, Err(e) => return Err(format!("get clipboard: {}", e).into()) };
        Ok(ClipboardData { content: plain_text, html: None, rtf: None, image: None, content_type: "text".to_string(), timestamp })
    }
    fn has_content_changed(&mut self, data: &ClipboardData, from_server: bool, server_timestamp: Option<u64>) -> bool {
        let current_hash = Self::calculate_content_hash(data);
        if from_server { if let Some(ts) = server_timestamp { self.last_server_timestamp = Some(ts); } }
        if let Some(last) = self.last_content_hash { if last == current_hash { return false; } }
        if !from_server { if let Some(server_ts) = self.last_server_timestamp { if data.timestamp <= server_ts + 5 { return false; } }}
        if !from_server { self.last_content_hash = Some(current_hash); }
        true
    }
    fn mark_content_as_sent(&mut self, data: &ClipboardData) { self.last_sent_hash = Some(Self::calculate_content_hash(data)); }
    fn is_own_content_returned(&self, data: &ClipboardData) -> bool { self.last_sent_hash.map_or(false, |h| h == Self::calculate_content_hash(data)) }
    fn set_clipboard_data_from_server(&mut self, data: &ClipboardData) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        set_clipboard(formats::Unicode, &data.content).map_err(|e| format!("set clipboard: {}", e))?;
        self.last_content_hash = Some(Self::calculate_content_hash(data));
        self.last_server_timestamp = Some(data.timestamp);
        debug!("Set text content on Windows: {} chars", data.content.len());
        Ok(())
    }
}

