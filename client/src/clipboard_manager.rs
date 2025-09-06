use crate::ClipboardData;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

#[cfg(target_os = "linux")]
use wl_clipboard_rs::{
    copy::{copy_multi, MimeSource, MimeType as CopyMimeType, Options, Source},
    paste::{get_contents, ClipboardType, Seat}
};

#[cfg(target_os = "windows")]
use std::process::Command;

/// Cross-platform clipboard manager using native APIs
/// - Linux: wl-clipboard-rs for Wayland/X11 support
/// - Windows: system commands for clipboard operations  
/// - Supports text and HTML formats with proper multi-format handling
pub struct ClipboardManager {
    /// Cache of last content hash to prevent infinite loops
    last_content_hash: Option<u64>,
    /// Timestamp of last server update to avoid conflicts
    last_server_timestamp: Option<u64>,
    /// Hash of last content sent to server to detect own content
    last_sent_hash: Option<u64>,
}

impl ClipboardManager {
    /// Create a new clipboard manager instance
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            last_content_hash: None,
            last_server_timestamp: None,
            last_sent_hash: None,
        })
    }

    /// Calculate content hash for change detection
    /// Normalizes HTML vs plain text to avoid ping-pong cycles
    fn calculate_content_hash(data: &ClipboardData) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        
        // Normalize content: if plain text looks like HTML tags,
        // normalize it for comparison with HTML version
        let normalized_content = if data.content.trim().starts_with('<') && data.content.trim().ends_with('>') {
            // If plain text contains HTML tags, use them for hashing
            data.content.trim().to_string()
        } else {
            data.content.clone()
        };
        
        // Hash normalized content
        normalized_content.hash(&mut hasher);
        
        // Hash HTML if it exists and differs from plain text
        if let Some(ref html) = data.html {
            if html.trim() != normalized_content.trim() {
                html.hash(&mut hasher);
            }
        }
        
        // Hash RTF content if present
        if let Some(ref rtf) = data.rtf {
            rtf.hash(&mut hasher);
        }
        
        hasher.finish()
    }
    
    /// Check if content has actually changed
    pub fn has_content_changed(&mut self, data: &ClipboardData, from_server: bool, server_timestamp: Option<u64>) -> bool {
        let current_hash = Self::calculate_content_hash(data);
        let source = if from_server { "server" } else { "local" };
        
        debug!("Checking {} content change: hash={}, last_hash={:?}, timestamp={}, last_server_ts={:?}", 
               source, current_hash, self.last_content_hash, data.timestamp, self.last_server_timestamp);
        
        // If data came from server, save timestamp
        if from_server {
            if let Some(timestamp) = server_timestamp {
                self.last_server_timestamp = Some(timestamp);
            }
        }
        
        // Check if hash changed
        if let Some(last_hash) = self.last_content_hash {
            if last_hash == current_hash {
                debug!("Content unchanged (same hash): {}", current_hash);
                return false;
            }
        }
        
        // For local changes: check timestamps (only if we have recent server data)
        if !from_server {
            if let Some(server_ts) = self.last_server_timestamp {
                // 5 second tolerance for stability
                if data.timestamp <= server_ts + 5 {
                    debug!("Ignoring local change too close to server update: {} <= {} (within 5s tolerance)", data.timestamp, server_ts + 5);
                    return false;
                }
                debug!("Local change is far enough from server update: {} > {} (outside 5s tolerance)", data.timestamp, server_ts + 5);
            }
        }
        
        // Update cache only for local changes
        if !from_server {
            debug!("Accepting local content change: updating hash from {:?} to {}", self.last_content_hash, current_hash);
            self.last_content_hash = Some(current_hash);
        } else {
            debug!("Server content - not updating hash cache here");
        }
        
        true
    }
    
    /// Mark that content was sent to server
    pub fn mark_content_as_sent(&mut self, data: &ClipboardData) {
        let hash = Self::calculate_content_hash(data);
        self.last_sent_hash = Some(hash);
        debug!("Marked content as sent to server: hash={}", hash);
    }
    
    /// Check if this is our own content returned from server
    pub fn is_own_content_returned(&self, data: &ClipboardData) -> bool {
        if let Some(last_sent) = self.last_sent_hash {
            let current_hash = Self::calculate_content_hash(data);
            let is_own = last_sent == current_hash;
            if is_own {
                debug!("Detected own content returned from server: hash={}", current_hash);
            }
            is_own
        } else {
            false
        }
    }

    /// Public wrapper for getting clipboard data
    pub fn get_clipboard_data(&self) -> Result<ClipboardData, Box<dyn std::error::Error + Send + Sync>> {
        self.get_clipboard_data_internal()
    }

    /// Read clipboard data from system clipboard
    pub fn get_clipboard_data_internal(&self) -> Result<ClipboardData, Box<dyn std::error::Error + Send + Sync>> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();

        // Get plain text content
        let plain_text = self.get_text_content()?;
        
        // Get HTML content if available
        let html_content = self.get_html_content().ok();
        
        // Get RTF content if available (Windows only for now)
        let rtf_content = self.get_rtf_content().ok();

        // Determine content type based on what we found
        let content_type = if html_content.is_some() {
            if rtf_content.is_some() {
                debug!("Found both HTML and RTF content");
                "mixed".to_string()
            } else {
                debug!("Found HTML content");
                "html".to_string()
            }
        } else if rtf_content.is_some() {
            debug!("Found RTF content");
            "rtf".to_string()
        } else {
            debug!("Plain text content only");
            "text".to_string()
        };

        Ok(ClipboardData {
            content: plain_text,
            html: html_content,
            rtf: rtf_content,
            image: None, // Images not supported
            content_type,
            timestamp,
        })
    }

    /// Set clipboard data from server (with conflict detection)
    pub fn set_clipboard_data_from_server(&mut self, data: &ClipboardData) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Setting clipboard from server: {} chars, type: {}", data.content.len(), data.content_type);
        
        // Set clipboard content
        let result = self.set_clipboard_content(data);
        
        if result.is_ok() {
            // After successful setting, read ACTUAL clipboard content
            match self.get_clipboard_data_internal() {
                Ok(actual_data) => {
                    // Update cache with actual content
                    let actual_hash = Self::calculate_content_hash(&actual_data);
                    self.last_content_hash = Some(actual_hash);
                    self.last_server_timestamp = Some(data.timestamp);
                    
                    debug!("Updated cache with ACTUAL clipboard content: hash={}, type={}, timestamp={}", 
                           actual_hash, actual_data.content_type, data.timestamp);
                }
                Err(e) => {
                    warn!("Failed to read actual clipboard content after setting: {}", e);
                    // Fallback - use original data
                    let fallback_hash = Self::calculate_content_hash(data);
                    self.last_content_hash = Some(fallback_hash);
                    self.last_server_timestamp = Some(data.timestamp);
                }
            }
            
            debug!("Successfully set clipboard from server");
        } else {
            debug!("Failed to set clipboard from server");
        }
        
        result
    }

    /// Set clipboard content (internal method)
    fn set_clipboard_content(&self, data: &ClipboardData) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if data.content.is_empty() {
            return Ok(()); // Nothing to set
        }

        // On Linux, use wl-clipboard-rs for all operations
        #[cfg(target_os = "linux")]
        {
            if let Some(ref html) = data.html {
                debug!("Setting multi-format content: {} chars plain text, {} chars html", data.content.len(), html.len());
                
                // Set both plain text and HTML simultaneously
                if let Err(e) = self.set_multi_format_content(&data.content, html) {
                    warn!("Failed to set multi-format clipboard: {}", e);
                    // Fallback to text-only
                    self.set_text_content(&data.content)?;
                } else {
                    debug!("Successfully set both plain text and HTML via wl-clipboard-rs");
                }
            } else {
                // Text-only content
                self.set_text_content(&data.content)?;
            }
        }

        // On Windows, use system commands
        #[cfg(target_os = "windows")]
        {
            // Set plain text first
            self.set_text_content(&data.content)?;
            
            // Add HTML format if available
            if let Some(ref html) = data.html {
                debug!("Setting HTML format: {} chars", html.len());
                if let Err(e) = self.set_html_content(html) {
                    warn!("Failed to set HTML format: {}", e);
                }
            }
            
            // Add RTF format if available
            if let Some(ref rtf) = data.rtf {
                debug!("Setting RTF format: {} chars", rtf.len());
                if let Err(e) = self.set_rtf_content(rtf) {
                    warn!("Failed to set RTF format: {}", e);
                }
            }
        }

        // On other platforms, just set text
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            return Err("Clipboard operations not supported on this platform".into());
        }

        Ok(())
    }

    // Platform-specific implementations

    /// Get plain text content from clipboard
    #[cfg(target_os = "linux")]
    fn get_text_content(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        use wl_clipboard_rs::paste::MimeType;
        match get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Text) {
            Ok((mut data, _)) => {
                use std::io::Read;
                let mut contents = String::new();
                data.read_to_string(&mut contents)?;
                Ok(contents.trim_end().to_string())
            }
            Err(e) => {
                debug!("Failed to get text via wl-clipboard-rs: {}", e);
                Err(e.into())
            }
        }
    }

    /// Get HTML content from clipboard
    #[cfg(target_os = "linux")]
    fn get_html_content(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        use wl_clipboard_rs::paste::MimeType;
        match get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Specific("text/html")) {
            Ok((mut data, _)) => {
                use std::io::Read;
                let mut contents = String::new();
                data.read_to_string(&mut contents)?;
                Ok(contents)
            }
            Err(e) => {
                debug!("No HTML content available: {}", e);
                Err(e.into())
            }
        }
    }

    /// Get RTF content from clipboard (not supported on Linux yet)
    #[cfg(target_os = "linux")]
    fn get_rtf_content(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Err("RTF not supported on Linux yet".into())
    }

    /// Set text content to clipboard
    #[cfg(target_os = "linux")]
    fn set_text_content(&self, text: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let opts = Options::new();
        let source = MimeSource {
            source: Source::Bytes(text.as_bytes().to_vec().into_boxed_slice()),
            mime_type: CopyMimeType::Text,
        };
        
        wl_clipboard_rs::copy::copy_multi(opts, vec![source])?;
        debug!("Successfully set text content: {} chars", text.len());
        Ok(())
    }

    /// Set multi-format clipboard content (text + HTML)
    #[cfg(target_os = "linux")]
    fn set_multi_format_content(&self, plain_text: &str, html: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Setting multi-format clipboard: {} chars plain, {} chars html", plain_text.len(), html.len());
        
        let opts = Options::new();
        let sources = vec![
            MimeSource {
                source: Source::Bytes(plain_text.as_bytes().to_vec().into_boxed_slice()),
                mime_type: CopyMimeType::Text,
            },
            MimeSource {
                source: Source::Bytes(html.as_bytes().to_vec().into_boxed_slice()),
                mime_type: CopyMimeType::Specific("text/html".to_string()),
            },
        ];
        
        copy_multi(opts, sources)?;
        debug!("Successfully set both plain text and HTML formats via wl-clipboard-rs");
        Ok(())
    }

    // Windows implementations (using system commands for now)
    #[cfg(target_os = "windows")]
    fn get_text_content(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Simple implementation using PowerShell for now
        let output = Command::new("powershell")
            .args(["-Command", "Get-Clipboard"])
            .output()?;

        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            Ok(text.trim_end().to_string())
        } else {
            Err("Failed to get clipboard text on Windows".into())
        }
    }

    #[cfg(target_os = "windows")]
    fn get_html_content(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // HTML clipboard support would require Windows API calls
        // For now, return error
        Err("HTML clipboard reading not implemented on Windows yet".into())
    }

    #[cfg(target_os = "windows")]
    fn get_rtf_content(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // RTF clipboard support would require Windows API calls
        // For now, return error
        Err("RTF clipboard reading not implemented on Windows yet".into())
    }

    #[cfg(target_os = "windows")]
    fn set_text_content(&self, text: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut child = Command::new("powershell")
            .args(["-Command", &format!("Set-Clipboard -Value '{}'", text.replace("'", "''"))])
            .spawn()?;

        let status = child.wait()?;
        if status.success() {
            debug!("Successfully set text content on Windows: {} chars", text.len());
            Ok(())
        } else {
            Err("Failed to set clipboard text on Windows".into())
        }
    }

    #[cfg(target_os = "windows")]
    fn set_html_content(&self, _html: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // HTML clipboard support would require Windows API calls
        warn!("HTML clipboard setting not implemented on Windows yet");
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn set_rtf_content(&self, _rtf: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // RTF clipboard support would require Windows API calls
        warn!("RTF clipboard setting not implemented on Windows yet");
        Ok(())
    }
}
