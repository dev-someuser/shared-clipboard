use crate::ClipboardData;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, warn};
#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use wl_clipboard_rs::{copy::{MimeSource, MimeType, Options, Source}};

pub struct ClipboardManager {
    // Пока только arboard, в будущем добавим rich text поддержку
    arboard: arboard::Clipboard,
    // Кэш последних данных для предотвращения циклов
    last_content_hash: Option<u64>,
    last_server_timestamp: Option<u64>,
    // Кэш последнего отправленного на сервер контента
    last_sent_hash: Option<u64>,
}

impl ClipboardManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            arboard: arboard::Clipboard::new()?,
            last_content_hash: None,
            last_server_timestamp: None,
            last_sent_hash: None,
        })
    }
    
    /// Вычисляем хэш содержимого для определения изменений
    /// Нормализует HTML vs plain text чтобы избежать ping-pong циклов
    fn calculate_content_hash(data: &ClipboardData) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        
        // Основная логика: если контент выглядит как HTML теги в plain text,
        // нормализуем его для сравнения с HTML версией
        let normalized_content = if data.content.trim().starts_with('<') && data.content.trim().ends_with('>') {
            // Если plain text содержит HTML теги, используем их для хеширования
            data.content.trim().to_string()
        } else {
            data.content.clone()
        };
        
        // Хешируем нормализованный контент
        normalized_content.hash(&mut hasher);
        
        // Если есть HTML, хешируем его тоже, но только если он отличается от plain text
        if let Some(ref html) = data.html {
            if html.trim() != normalized_content.trim() {
                html.hash(&mut hasher);
            }
        }
        
        // RTF и изображения хешируем как обычно
        if let Some(ref rtf) = data.rtf {
            rtf.hash(&mut hasher);
        }
        if let Some(ref image) = data.image {
            image.hash(&mut hasher);
        }
        
        hasher.finish()
    }
    
    /// Проверяем, действительно ли изменилось содержимое
    pub fn has_content_changed(&mut self, data: &ClipboardData, from_server: bool, server_timestamp: Option<u64>) -> bool {
        let current_hash = Self::calculate_content_hash(data);
        let source = if from_server { "server" } else { "local" };
        
        debug!("Checking {} content change: hash={}, last_hash={:?}, timestamp={}, last_server_ts={:?}", 
               source, current_hash, self.last_content_hash, data.timestamp, self.last_server_timestamp);
        
        // Если данные пришли с сервера, сохраняем таймстамп
        if from_server {
            if let Some(timestamp) = server_timestamp {
                self.last_server_timestamp = Some(timestamp);
            }
        }
        
        // Проверяем, изменился ли хэш
        if let Some(last_hash) = self.last_content_hash {
            if last_hash == current_hash {
                // Содержимое не изменилось
                debug!("Content unchanged (same hash): {}", current_hash);
                return false;
            }
        }
        
        // Для локальных изменений: проверяем временные метки (только если есть последние серверные данные)
        if !from_server {
            if let Some(server_ts) = self.last_server_timestamp {
                // Увеличиваем допуск до 5 секунд для большей стабильности
                if data.timestamp <= server_ts + 5 {
                    debug!("Ignoring local change too close to server update: {} <= {} (within 5s tolerance)", data.timestamp, server_ts + 5);
                    return false;
                }
                debug!("Local change is far enough from server update: {} > {} (outside 5s tolerance)", data.timestamp, server_ts + 5);
            }
        }
        
        // Обновляем кэш только для локальных изменений
        if !from_server {
            debug!("Accepting local content change: updating hash from {:?} to {}", self.last_content_hash, current_hash);
            self.last_content_hash = Some(current_hash);
        } else {
            debug!("Server content - not updating hash cache here");
        }
        
        true
    }
    
    /// Отмечаем, что контент был отправлен на сервер
    pub fn mark_content_as_sent(&mut self, data: &ClipboardData) {
        let hash = Self::calculate_content_hash(data);
        self.last_sent_hash = Some(hash);
        debug!("Marked content as sent to server: hash={}", hash);
    }
    
    /// Проверяем, не является ли это нашим собственным контентом, вернувшимся от сервера
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
    
    /// Linux-specific clipboard reading with Wayland/X11 detection
    #[cfg(target_os = "linux")]
    fn get_clipboard_via_system(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Detect if we're on Wayland or X11
        let is_wayland = std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland" ||
                        std::env::var("WAYLAND_DISPLAY").is_ok();
        
        if is_wayland {
            // Use wl-clipboard for Wayland
            let output = Command::new("wl-paste")
                .output()?;
            
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout).to_string();
                // wl-paste often adds trailing newlines, trim them to avoid infinite loops
                Ok(text.trim_end().to_string())
            } else {
                Err("wl-paste failed to read clipboard".into())
            }
        } else {
            // Use xclip for X11
            let output = Command::new("xclip")
                .args(["-o", "-selection", "clipboard"])
                .output()?;
            
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout).to_string();
                // Trim trailing whitespace for consistency
                Ok(text.trim_end().to_string())
            } else {
                Err("xclip failed to read clipboard".into())
            }
        }
    }
    
    /// Linux-specific clipboard writing with Wayland/X11 detection
    #[cfg(target_os = "linux")]
    fn set_clipboard_via_system(&self, text: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Detect if we're on Wayland or X11
        let is_wayland = std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland" ||
                        std::env::var("WAYLAND_DISPLAY").is_ok();
        
        if is_wayland {
            // Use wl-clipboard for Wayland
            let mut child = Command::new("wl-copy")
                .stdin(std::process::Stdio::piped())
                .spawn()?;
            
            if let Some(stdin) = child.stdin.as_mut() {
                use std::io::Write;
                stdin.write_all(text.as_bytes())?;
            }
            
            let status = child.wait()?;
            if status.success() {
                Ok(())
            } else {
                Err("wl-copy failed to write clipboard".into())
            }
        } else {
            // Use xclip for X11
            let mut child = Command::new("xclip")
                .args(["-i", "-selection", "clipboard"])
                .stdin(std::process::Stdio::piped())
                .spawn()?;
            
            if let Some(stdin) = child.stdin.as_mut() {
                use std::io::Write;
                stdin.write_all(text.as_bytes())?;
            }
            
            let status = child.wait()?;
            if status.success() {
                Ok(())
            } else {
                Err("xclip failed to write clipboard".into())
            }
        }
    }
    
    /// Linux-specific image clipboard reading with Wayland/X11 detection
    #[cfg(target_os = "linux")]
    fn get_image_via_system(&self) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        // Detect if we're on Wayland or X11
        let is_wayland = std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland" ||
                        std::env::var("WAYLAND_DISPLAY").is_ok();
        
        if is_wayland {
            // Check if clipboard has image content
            let list_output = Command::new("wl-paste")
                .args(["--list-types"])
                .output()?;
            
            if list_output.status.success() {
                let types = String::from_utf8_lossy(&list_output.stdout);
                debug!("Available clipboard types: {}", types.trim());
                
                // Look for image types
                if types.contains("image/png") || types.contains("image/jpeg") || types.contains("image/") {
                    // Get image as PNG bytes
                    let image_output = Command::new("wl-paste")
                        .args(["--type", "image/png"])
                        .output();
                    
                    match image_output {
                        Ok(output) if output.status.success() && !output.stdout.is_empty() => {
                            use base64::{Engine as _, engine::general_purpose};
                            // For now, we don't have image dimensions, use placeholder
                            let base64_data = general_purpose::STANDARD.encode(&output.stdout);
                            let image_info = format!("{}:{}:{}", 0, 0, base64_data);
                            debug!("Got image via wl-paste: {} bytes", output.stdout.len());
                            return Ok(Some(image_info));
                        }
                        Ok(_) => debug!("wl-paste returned empty or failed for image"),
                        Err(e) => debug!("wl-paste image command failed: {}", e),
                    }
                }
            }
        }
        // TODO: Add xclip image support for X11 if needed
        Ok(None)
    }
    
    /// Linux-specific HTML clipboard reading with Wayland/X11 detection
    #[cfg(target_os = "linux")]
    fn get_html_via_system(&self) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        let is_wayland = std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland" ||
                        std::env::var("WAYLAND_DISPLAY").is_ok();
        
        if is_wayland {
            // Check if clipboard has HTML content
            let list_output = Command::new("wl-paste")
                .args(["--list-types"])
                .output()?;
            
            if list_output.status.success() {
                let types = String::from_utf8_lossy(&list_output.stdout);
                
                if types.contains("text/html") {
                    let html_output = Command::new("wl-paste")
                        .args(["--type", "text/html"])
                        .output();
                    
                    match html_output {
                        Ok(output) if output.status.success() && !output.stdout.is_empty() => {
                            let html = String::from_utf8_lossy(&output.stdout).trim_end().to_string();
                            debug!("Got HTML via wl-paste: {} chars", html.len());
                            return Ok(Some(html));
                        }
                        Ok(_) => debug!("wl-paste returned empty or failed for HTML"),
                        Err(e) => debug!("wl-paste HTML command failed: {}", e),
                    }
                }
            }
        }
        // TODO: Add xclip HTML support for X11 if needed
        Ok(None)
    }
    
    /// Linux-specific RTF clipboard reading with Wayland/X11 detection
    #[cfg(target_os = "linux")]
    fn get_rtf_via_system(&self) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        let is_wayland = std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland" ||
                        std::env::var("WAYLAND_DISPLAY").is_ok();
        
        if is_wayland {
            // Check if clipboard has RTF content
            let list_output = Command::new("wl-paste")
                .args(["--list-types"])
                .output()?;
            
            if list_output.status.success() {
                let types = String::from_utf8_lossy(&list_output.stdout);
                
                if types.contains("application/rtf") || types.contains("text/rtf") {
                    // Try application/rtf first, then text/rtf
                    let rtf_types = ["application/rtf", "text/rtf"];
                    
                    for rtf_type in &rtf_types {
                        let rtf_output = Command::new("wl-paste")
                            .args(["--type", rtf_type])
                            .output();
                        
                        match rtf_output {
                            Ok(output) if output.status.success() && !output.stdout.is_empty() => {
                                let rtf = String::from_utf8_lossy(&output.stdout).trim_end().to_string();
                                debug!("Got RTF via wl-paste ({}): {} chars", rtf_type, rtf.len());
                                return Ok(Some(rtf));
                            }
                            Ok(_) => debug!("wl-paste returned empty for RTF type: {}", rtf_type),
                            Err(e) => debug!("wl-paste RTF command failed for {}: {}", rtf_type, e),
                        }
                    }
                }
            }
        }
        // TODO: Add xclip RTF support for X11 if needed
        Ok(None)
    }

    /// Получить все доступные форматы из буфера обмена
    pub fn get_clipboard_data(&mut self) -> Result<ClipboardData, Box<dyn std::error::Error + Send + Sync>> {
        self.get_clipboard_data_internal()
    }
    
    /// Проверяем, изменился ли локальный буфер обмена
    pub fn check_local_clipboard_changed(&mut self) -> Result<Option<ClipboardData>, Box<dyn std::error::Error + Send + Sync>> {
        let data = self.get_clipboard_data_internal()?;
        
        if self.has_content_changed(&data, false, None) {
            debug!("Local clipboard changed: {} chars + rich content, type: {}", data.content.len(), data.content_type);
            if data.html.is_some() {
                debug!("  - Has HTML content");
            }
            if data.rtf.is_some() {
                debug!("  - Has RTF content");
            }
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }
    
    /// Внутренняя функция получения данных
    fn get_clipboard_data_internal(&mut self) -> Result<ClipboardData, Box<dyn std::error::Error + Send + Sync>> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // На Linux используем xclip как основной метод, на других ОС - arboard
        let plain_text = {
            #[cfg(target_os = "linux")]
            {
                // На Linux приоритет wl-clipboard/xclip - они более надёжные
                match self.get_clipboard_via_system() {
                    Ok(text) => {
                        debug!("Got text via system clipboard: {} chars", text.len());
                        text
                    }
                    Err(xe) => {
                        debug!("System clipboard failed, trying arboard: {}", xe);
                        match self.arboard.get_text() {
                            Ok(text) => {
                                debug!("Got text via arboard fallback: {} chars", text.len());
                                text
                            }
                            Err(ae) => {
                                debug!("Both system clipboard and arboard failed: system={}, arboard={}", xe, ae);
                                String::new()
                            }
                        }
                    }
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                match self.arboard.get_text() {
                    Ok(text) => text,
                    Err(e) => {
                        debug!("Failed to get plain text via arboard: {}", e);
                        String::new()
                    }
                }
            }
        };

        // Получаем HTML и RTF через system clipboard
        let html_content = self.get_html_via_system().unwrap_or(None);
        let rtf_content = self.get_rtf_via_system().unwrap_or(None);

        // TODO: Изображения временно отключены для стабильности
        let image_data = None;
        debug!("Image support temporarily disabled");

        // Определяем тип контента (изображения отключены)
        let (final_content, content_type) = if html_content.is_some() {
            if rtf_content.is_some() {
                debug!("Found both HTML and RTF content");
                (plain_text, "mixed".to_string())
            } else {
                debug!("Found HTML content");
                (plain_text, "html".to_string())
            }
        } else if rtf_content.is_some() {
            debug!("Found RTF content");
            (plain_text, "rtf".to_string())
        } else {
            debug!("Plain text content only");
            (plain_text, "text".to_string())
        };

        Ok(ClipboardData {
            content: final_content,
            html: html_content,
            rtf: rtf_content,
            image: image_data,
            content_type,
            timestamp,
        })
    }

    /// Установить данные в буфер обмена (с проверкой изменений)
    pub fn set_clipboard_data(&mut self, data: &ClipboardData) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.set_clipboard_data_internal(data, false, None)
    }
    
    /// Установить данные от сервера (с проверкой изменений)
    pub fn set_clipboard_data_from_server(&mut self, data: &ClipboardData) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Setting clipboard from server: {} chars, type: {}", data.content.len(), data.content_type);
        
        // Устанавливаем данные в буфер
        let result = self.set_clipboard_data_internal(data, true, Some(data.timestamp));
        
        if result.is_ok() {
            // После успешной установки читаем ФАКТИЧЕСКОЕ содержимое буфера
            match self.get_clipboard_data_internal() {
                Ok(actual_data) => {
                    // Обновляем кэш с фактическим содержимым
                    let actual_hash = Self::calculate_content_hash(&actual_data);
                    self.last_content_hash = Some(actual_hash);
                    self.last_server_timestamp = Some(data.timestamp);
                    
                    debug!("Updated cache with ACTUAL clipboard content: hash={}, type={}, timestamp={}", 
                           actual_hash, actual_data.content_type, data.timestamp);
                }
                Err(e) => {
                    warn!("Failed to read actual clipboard content after setting: {}", e);
                    // Fallback - используем оригинальные данные
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
    
    /// Внутренняя функция установки данных
    fn set_clipboard_data_internal(&mut self, data: &ClipboardData, from_server: bool, server_timestamp: Option<u64>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Setting clipboard data of type: {}", data.content_type);

        // На Linux всегда используем system clipboard (wl-copy/xclip), на других ОС - arboard
        if !data.content.is_empty() {
            #[cfg(target_os = "linux")]
            {
                // На Linux используем только wl-copy/xclip - без arboard из-за проблем с фокусом
                if let Err(e) = self.set_clipboard_via_system(&data.content) {
                    error!("Failed to set plain text via system clipboard: {}", e);
                } else {
                    debug!("Successfully set plain text via system clipboard: {} chars", data.content.len());
                }
            }
            
            #[cfg(not(target_os = "linux"))]
            {
                // На других ОС используем arboard
                if let Err(e) = self.arboard.set_text(&data.content) {
                    warn!("Failed to set plain text via arboard: {}", e);
                } else {
                    debug!("Successfully set text via arboard: {} chars", data.content.len());
                }
            }
        }

        // На Windows устанавливаем форматы после plain text
        #[cfg(target_os = "windows")]
        {
            // На Windows добавляем HTML/RTF форматы к уже установленному plain text
            if let Some(ref html) = data.html {
                debug!("Setting HTML format: {} chars", html.len());
                if let Err(e) = self.set_html_via_system(html) {
                    warn!("Failed to set HTML format: {}", e);
                }
            }
            
            if let Some(ref rtf) = data.rtf {
                debug!("Setting RTF format: {} chars", rtf.len());
                if let Err(e) = self.set_rtf_via_system(rtf) {
                    warn!("Failed to set RTF format: {}", e);
                }
            }
        }
        
        // На Linux правильно устанавливаем rich форматы
        #[cfg(target_os = "linux")]
        {
            if let Some(ref html) = data.html {
                debug!("Setting HTML content: {} chars plain text, {} chars html", data.content.len(), html.len());
                
                // Используем wl-clipboard-rs для установки нескольких MIME типов одновременно
                if let Err(e) = self.set_multi_format_clipboard(&data.content, html) {
                    warn!("Failed to set multi-format clipboard: {}", e);
                    // Fallback к старому методу
                    if let Err(e2) = self.set_html_via_system(html) {
                        warn!("Fallback HTML setting also failed: {}", e2);
                    }
                } else {
                    debug!("Successfully set both plain text and HTML via wl-clipboard-rs");
                }
            }
            
            if let Some(ref rtf) = data.rtf {
                debug!("Setting RTF content: {} chars", rtf.len());
                if let Err(e) = self.set_rtf_via_system(rtf) {
                    warn!("Failed to set RTF: {}", e);
                }
            }
        }

        // Устанавливаем изображение если доступно
        if let Some(ref image_info) = data.image {
            // Парсим формат: width:height:base64_data
            let parts: Vec<&str> = image_info.splitn(3, ':').collect();
            if parts.len() == 3 {
                if let (Ok(width), Ok(height)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                    use base64::{Engine as _, engine::general_purpose};
                    match general_purpose::STANDARD.decode(parts[2]) {
                        Ok(image_bytes) => {
                            let image_data = arboard::ImageData {
                                width,
                                height,
                                bytes: image_bytes.into(),
                            };
                            if let Err(e) = self.arboard.set_image(image_data) {
                                warn!("Failed to set image: {}", e);
                            } else {
                                debug!("Successfully set image: {}x{}", width, height);
                            }
                        }
                        Err(e) => {
                            error!("Failed to decode image base64: {}", e);
                        }
                    }
                } else {
                    error!("Invalid image dimensions in: {}", image_info);
                }
            } else {
                error!("Invalid image format, expected width:height:data");
            }
        }

        Ok(())
    }
    
    /// Установка нескольких MIME типов одновременно через wl-clipboard-rs
    #[cfg(target_os = "linux")]
    fn set_multi_format_clipboard(&self, plain_text: &str, html: &str) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Setting multi-format clipboard: {} chars plain, {} chars html", plain_text.len(), html.len());
        
        // Создаем опции для операции
        let opts = Options::new();
        
        // Создаем источники данных для обоих MIME типов
        let sources = vec![
            MimeSource {
                source: Source::Bytes(plain_text.as_bytes().to_vec().into_boxed_slice()),
                mime_type: MimeType::Text,
            },
            MimeSource {
                source: Source::Bytes(html.as_bytes().to_vec().into_boxed_slice()),
                mime_type: MimeType::Specific("text/html".to_string()),
            },
        ];
        
        // Устанавливаем в буфер обе MIME типа одновременно
        wl_clipboard_rs::copy::copy_multi(opts, sources)?;
        
        debug!("Successfully set both plain text and HTML formats via wl-clipboard-rs");
        Ok(())
    }
    
    /// Fallback для не-Linux систем
    #[cfg(not(target_os = "linux"))]
    fn set_multi_format_clipboard(&self, _plain_text: &str, _html: &str) -> Result<(), Box<dyn std::error::Error>> {
        // На не-Linux системах используем обычную логику
        Err("Multi-format clipboard not supported on non-Linux systems".into())
    }
    
    /// Экстракция plain text из HTML для правильного сохранения в буфер
    fn extract_plain_text_from_html(&self, html: &str) -> String {
        // Простая экстракция текста из HTML (убираем теги)
        let mut result = String::new();
        let mut inside_tag = false;
        let mut chars = html.chars();
        
        while let Some(ch) = chars.next() {
            match ch {
                '<' => inside_tag = true,
                '>' => inside_tag = false,
                _ if !inside_tag => result.push(ch),
                _ => {} // Игнорируем символы внутри тегов
            }
        }
        
        // Очищаем лишние пробелы и переводы строк
        result
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join(" ")
            .trim()
            .to_string()
    }
    
    #[cfg(target_os = "linux")]
    fn set_html_via_system(&self, html: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Эксперимент: пробуем установить HTML через wl-copy --type text/html
        debug!("Attempting to set HTML format: {} chars", html.len());
        
        let mut child = std::process::Command::new("wl-copy")
            .args(&["--type", "text/html"])
            .stdin(std::process::Stdio::piped())
            .spawn()?;
            
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(html.as_bytes())?;
            stdin.flush()?;
        }
        
        let status = child.wait()?;
        if !status.success() {
            return Err(format!("wl-copy HTML failed with status: {}", status).into());
        }
        
        debug!("Set HTML format via wl-copy: {} chars (may have overwritten plain text)", html.len());
        Ok(())
    }
    
    #[cfg(target_os = "linux")]
    fn set_rtf_via_system(&self, rtf: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut child = std::process::Command::new("wl-copy")
            .args(&["--type", "application/rtf"])
            .stdin(std::process::Stdio::piped())
            .spawn()?;
            
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(rtf.as_bytes())?;
            stdin.flush()?;
        }
        
        let status = child.wait()?;
        if !status.success() {
            return Err(format!("wl-copy failed with status: {}", status).into());
        }
        
        debug!("Successfully set RTF content via wl-copy: {} chars", rtf.len());
        Ok(())
    }
    
    /// Windows-specific HTML clipboard reading
    #[cfg(target_os = "windows")]
    fn get_html_via_system(&self) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        use winapi::um::winuser::{OpenClipboard, CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, RegisterClipboardFormatW};
        use winapi::um::winbase::GlobalLock;
        use winapi::um::errhandlingapi::GetLastError;
        use std::ptr::null_mut;
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        
        unsafe {
            // Register HTML format
            let html_format_name: Vec<u16> = OsStr::new("HTML Format")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let html_format = RegisterClipboardFormatW(html_format_name.as_ptr());
            
            if html_format == 0 {
                return Ok(None);
            }
            
            if OpenClipboard(null_mut()) == 0 {
                debug!("Failed to open clipboard for HTML reading: {}", GetLastError());
                return Ok(None);
            }
            
            let result = if IsClipboardFormatAvailable(html_format) != 0 {
                let handle = GetClipboardData(html_format);
                if !handle.is_null() {
                    let data_ptr = GlobalLock(handle) as *const u8;
                    if !data_ptr.is_null() {
                        // HTML Format has a specific structure, extract the HTML part
                        let data_slice = std::slice::from_raw_parts(data_ptr, 8192); // Reasonable limit
                        let html_data = std::ffi::CStr::from_ptr(data_ptr as *const i8);
                        let html_string = html_data.to_string_lossy();
                        
                        // Parse HTML Format to extract actual HTML, avoid nested formats
                        if html_string.contains("StartHTML:") && html_string.contains("EndHTML:") {
                            // This is a proper HTML Format, extract the fragment
                            if let Some(fragment_start) = html_string.find("<!--StartFragment-->") {
                                if let Some(fragment_end) = html_string.find("<!--EndFragment-->") {
                                    let start_pos = fragment_start + "<!--StartFragment-->".len();
                                    let html_content = html_string[start_pos..fragment_end].trim().to_string();
                                    if !html_content.is_empty() && !html_content.contains("<!--StartFragment-->") {
                                        debug!("Got HTML fragment via Windows API: {} chars", html_content.len());
                                        Some(html_content)
                                    } else {
                                        None // Avoid nested or empty fragments
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else if html_string.contains("<html") && html_string.contains("</html>") {
                            // Simple HTML without HTML Format wrapper
                            Some(html_string.trim().to_string())
                        } else {
                            None // Not valid HTML
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            
            CloseClipboard();
            Ok(result)
        }
    }
    
    /// Windows-specific RTF clipboard reading
    #[cfg(target_os = "windows")]
    fn get_rtf_via_system(&self) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        use winapi::um::winuser::{OpenClipboard, CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, RegisterClipboardFormatW};
        use winapi::um::winbase::GlobalLock;
        use winapi::um::errhandlingapi::GetLastError;
        use std::ptr::null_mut;
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        
        unsafe {
            // Register RTF format
            let rtf_format_name: Vec<u16> = OsStr::new("Rich Text Format")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let rtf_format = RegisterClipboardFormatW(rtf_format_name.as_ptr());
            
            if rtf_format == 0 {
                return Ok(None);
            }
            
            if OpenClipboard(null_mut()) == 0 {
                debug!("Failed to open clipboard for RTF reading: {}", GetLastError());
                return Ok(None);
            }
            
            let result = if IsClipboardFormatAvailable(rtf_format) != 0 {
                let handle = GetClipboardData(rtf_format);
                if !handle.is_null() {
                    let data_ptr = GlobalLock(handle) as *const u8;
                    if !data_ptr.is_null() {
                        let rtf_data = std::ffi::CStr::from_ptr(data_ptr as *const i8);
                        let rtf_content = rtf_data.to_string_lossy().trim_end().to_string();
                        debug!("Got RTF via Windows API: {} chars", rtf_content.len());
                        Some(rtf_content)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            
            CloseClipboard();
            Ok(result)
        }
    }
    
    /// Windows-specific HTML clipboard writing
    #[cfg(target_os = "windows")]
    fn set_html_via_system(&self, html: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Let arboard handle the plain text, just add HTML format alongside
        self.set_html_format_only(html)
    }
    
    /// Windows helper to set HTML format without clearing clipboard
    #[cfg(target_os = "windows")]
    fn set_html_format_only(&self, html: &str) -> Result<(), Box<dyn std::error::Error>> {
        use winapi::um::winuser::{OpenClipboard, CloseClipboard, SetClipboardData, RegisterClipboardFormatW};
        use winapi::um::winbase::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
        use winapi::um::errhandlingapi::GetLastError;
        use std::ptr::null_mut;
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        
        unsafe {
            // Register HTML format
            let html_format_name: Vec<u16> = OsStr::new("HTML Format")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let html_format = RegisterClipboardFormatW(html_format_name.as_ptr());
            
            if html_format == 0 {
                return Err(format!("Failed to register HTML format: {}", GetLastError()).into());
            }
            
            // Check if HTML is already in HTML Format to avoid nesting
            let clean_html = if html.contains("<!--StartFragment-->") && html.contains("<!--EndFragment-->") {
                // Extract just the content between fragments to avoid nesting
                if let Some(start) = html.find("<!--StartFragment-->") {
                    if let Some(end) = html.find("<!--EndFragment-->") {
                        let content_start = start + "<!--StartFragment-->".len();
                        html[content_start..end].trim()
                    } else {
                        html.trim()
                    }
                } else {
                    html.trim()
                }
            } else {
                html.trim()
            };
            
            // Don't wrap if it's already wrapped HTML Format
            if clean_html.contains("StartHTML:") && clean_html.contains("EndHTML:") {
                debug!("HTML already in HTML Format, skipping wrapping");
                return Ok(()); // Don't add duplicate HTML Format
            }
            
            // Create HTML Format structure with proper offsets
            let start_fragment = 136;
            let end_fragment = start_fragment + clean_html.len();
            let start_html = 97;
            let end_html = end_fragment + 17; // </body></html>
            
            let html_format_data = format!(
                "Version:0.9\r\nStartHTML:{:08}\r\nEndHTML:{:08}\r\nStartFragment:{:08}\r\nEndFragment:{:08}\r\n<html><body>\r\n<!--StartFragment-->{}<!--EndFragment-->\r\n</body></html>",
                start_html,
                end_html,
                start_fragment,
                end_fragment,
                clean_html
            );
            
            if OpenClipboard(null_mut()) == 0 {
                return Err(format!("Failed to open clipboard: {}", GetLastError()).into());
            }
            
            // Don't empty clipboard - just add HTML format alongside existing formats
            
            let data_size = html_format_data.len() + 1;
            let mem_handle = GlobalAlloc(GMEM_MOVEABLE, data_size);
            if mem_handle.is_null() {
                CloseClipboard();
                return Err(format!("Failed to allocate memory: {}", GetLastError()).into());
            }
            
            let data_ptr = GlobalLock(mem_handle) as *mut u8;
            if data_ptr.is_null() {
                CloseClipboard();
                return Err(format!("Failed to lock memory: {}", GetLastError()).into());
            }
            
            std::ptr::copy_nonoverlapping(
                html_format_data.as_ptr(),
                data_ptr,
                html_format_data.len()
            );
            *data_ptr.add(html_format_data.len()) = 0; // Null terminator
            
            GlobalUnlock(mem_handle);
            
            if SetClipboardData(html_format, mem_handle).is_null() {
                CloseClipboard();
                return Err(format!("Failed to set clipboard data: {}", GetLastError()).into());
            }
            
            CloseClipboard();
            debug!("Successfully set HTML format via Windows API: {} chars", html.len());
            Ok(())
        }
    }
    
    /// Windows-specific RTF clipboard writing
    #[cfg(target_os = "windows")]
    fn set_rtf_via_system(&self, rtf: &str) -> Result<(), Box<dyn std::error::Error>> {
        use winapi::um::winuser::{OpenClipboard, CloseClipboard, SetClipboardData, RegisterClipboardFormatW};
        use winapi::um::winbase::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
        use winapi::um::errhandlingapi::GetLastError;
        use std::ptr::null_mut;
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        
        unsafe {
            // Register RTF format
            let rtf_format_name: Vec<u16> = OsStr::new("Rich Text Format")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let rtf_format = RegisterClipboardFormatW(rtf_format_name.as_ptr());
            
            if rtf_format == 0 {
                return Err(format!("Failed to register RTF format: {}", GetLastError()).into());
            }
            
            if OpenClipboard(null_mut()) == 0 {
                return Err(format!("Failed to open clipboard: {}", GetLastError()).into());
            }
            
            // Don't empty clipboard - just add RTF format alongside existing formats
            
            let data_size = rtf.len() + 1;
            let mem_handle = GlobalAlloc(GMEM_MOVEABLE, data_size);
            if mem_handle.is_null() {
                CloseClipboard();
                return Err(format!("Failed to allocate memory: {}", GetLastError()).into());
            }
            
            let data_ptr = GlobalLock(mem_handle) as *mut u8;
            if data_ptr.is_null() {
                CloseClipboard();
                return Err(format!("Failed to lock memory: {}", GetLastError()).into());
            }
            
            std::ptr::copy_nonoverlapping(
                rtf.as_ptr(),
                data_ptr,
                rtf.len()
            );
            *data_ptr.add(rtf.len()) = 0; // Null terminator
            
            GlobalUnlock(mem_handle);
            
            if SetClipboardData(rtf_format, mem_handle).is_null() {
                CloseClipboard();
                return Err(format!("Failed to set clipboard data: {}", GetLastError()).into());
            }
            
            CloseClipboard();
            debug!("Successfully set RTF format via Windows API: {} chars", rtf.len());
            Ok(())
        }
    }
    
    /// Stub implementations for unsupported platforms
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    fn get_html_via_system(&self) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    fn get_rtf_via_system(&self) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    fn set_html_via_system(&self, _html: &str) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    fn set_rtf_via_system(&self, _rtf: &str) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}
