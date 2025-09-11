use crate::ClipboardData;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::debug;
use super::ClipboardBackend;

use wl_clipboard_rs::{
    copy::{copy_multi, MimeSource, MimeType as CopyMimeType, Options, Source},
    paste::{get_contents, ClipboardType, Seat}
};

pub struct LinuxClipboardManager {
    last_content_hash: Option<u64>,
    last_server_timestamp: Option<u64>,
    last_sent_hash: Option<u64>,
}

impl LinuxClipboardManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self { last_content_hash: None, last_server_timestamp: None, last_sent_hash: None })
    }

    fn calculate_content_hash(data: &ClipboardData) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        let normalized_content = if data.content.trim().starts_with('<') && data.content.trim().ends_with('>') {
            data.content.trim().to_string()
        } else { data.content.clone() };
        normalized_content.hash(&mut hasher);
        if let Some(ref html) = data.html { if html.trim() != normalized_content.trim() { html.hash(&mut hasher); } }
        if let Some(ref rtf) = data.rtf { rtf.hash(&mut hasher); }
        hasher.finish()
    }

    fn get_text_content(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        use wl_clipboard_rs::paste::MimeType;
        match get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Text) {
            Ok((mut data, _)) => { use std::io::Read; let mut contents = String::new(); data.read_to_string(&mut contents)?; Ok(contents.trim_end().to_string()) }
            Err(e) => { debug!("Failed to get text via wl-clipboard-rs: {}", e); Err(e.into()) }
        }
    }
    fn get_html_content(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        use wl_clipboard_rs::paste::MimeType;
        match get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Specific("text/html")) {
            Ok((mut data, _)) => { use std::io::Read; let mut contents = String::new(); data.read_to_string(&mut contents)?; Ok(contents) }
            Err(e) => { debug!("No HTML content available: {}", e); Err(e.into()) }
        }
    }
    fn get_rtf_content(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> { Err("RTF not supported on Linux yet".into()) }

    fn set_text_content(&self, text: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let opts = Options::new();
        let source = MimeSource { source: Source::Bytes(text.as_bytes().to_vec().into_boxed_slice()), mime_type: CopyMimeType::Text };
        wl_clipboard_rs::copy::copy_multi(opts, vec![source])?;
        debug!("Successfully set text content: {} chars", text.len());
        Ok(())
    }
    fn set_multi_format_content(&self, plain_text: &str, html: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let opts = Options::new();
        let sources = vec![
            MimeSource { source: Source::Bytes(plain_text.as_bytes().to_vec().into_boxed_slice()), mime_type: CopyMimeType::Text },
            MimeSource { source: Source::Bytes(html.as_bytes().to_vec().into_boxed_slice()), mime_type: CopyMimeType::Specific("text/html".to_string()) },
        ];
        copy_multi(opts, sources)?; Ok(())
    }
}

impl ClipboardBackend for LinuxClipboardManager {
    fn get_clipboard_data(&self) -> Result<ClipboardData, Box<dyn std::error::Error + Send + Sync>> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let plain_text = self.get_text_content()?;
        let html_content = self.get_html_content().ok();
        let rtf_content = self.get_rtf_content().ok();
        let content_type = if html_content.is_some() { if rtf_content.is_some() { "mixed" } else { "html" } } else if rtf_content.is_some() { "rtf" } else { "text" }.to_string();
        Ok(ClipboardData { content: plain_text, html: html_content, rtf: rtf_content, image: None, content_type, timestamp })
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

    fn is_own_content_returned(&self, data: &ClipboardData) -> bool {
        self.last_sent_hash.map_or(false, |h| h == Self::calculate_content_hash(data))
    }

    fn set_clipboard_data_from_server(&mut self, data: &ClipboardData) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let result = if let Some(ref html) = data.html { self.set_multi_format_content(&data.content, html) } else { self.set_text_content(&data.content) };
        if result.is_ok() {
            self.last_content_hash = Some(Self::calculate_content_hash(data));
            self.last_server_timestamp = Some(data.timestamp);
        }
        result
    }
}

