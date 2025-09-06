use crate::ClipboardData;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, warn};

pub struct ClipboardManager {
    // Пока только arboard, в будущем добавим rich text поддержку
    arboard: arboard::Clipboard,
}

impl ClipboardManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            arboard: arboard::Clipboard::new()?,
        })
    }

    /// Получить все доступные форматы из буфера обмена
    pub fn get_clipboard_data(&mut self) -> Result<ClipboardData, Box<dyn std::error::Error + Send + Sync>> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Сначала пробуем получить plain text
        let plain_text = match self.arboard.get_text() {
            Ok(text) => text,
            Err(e) => {
                debug!("Failed to get plain text: {}", e);
                String::new()
            }
        };

        // TODO: Пока HTML и RTF не поддерживаются, можно добавить через другие библиотеки
        let html_content = None;
        let rtf_content = None;

        // Пробуем получить изображение
        let image_data = match self.arboard.get_image() {
            Ok(image) => {
                debug!("Got image: {}x{}", image.width, image.height);
                // Конвертируем в base64
                use base64::{Engine as _, engine::general_purpose};
                let base64_data = general_purpose::STANDARD.encode(&image.bytes);
                Some(base64_data)
            }
            Err(e) => {
                debug!("No image data available: {}", e);
                None
            }
        };

        // Определяем тип контента
        let content_type = if image_data.is_some() {
            if html_content.is_some() || rtf_content.is_some() {
                "mixed".to_string()
            } else {
                "image".to_string()
            }
        } else if html_content.is_some() {
            "html".to_string()
        } else if rtf_content.is_some() {
            "rtf".to_string()
        } else {
            "text".to_string()
        };

        Ok(ClipboardData {
            content: plain_text,
            html: html_content,
            rtf: rtf_content,
            image: image_data,
            content_type,
            timestamp,
        })
    }

    /// Установить данные в буфер обмена
    pub fn set_clipboard_data(&mut self, data: &ClipboardData) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Setting clipboard data of type: {}", data.content_type);

        // Всегда устанавливаем plain text
        if !data.content.is_empty() {
            if let Err(e) = self.arboard.set_text(&data.content) {
                warn!("Failed to set plain text: {}", e);
            }
        }

        // TODO: Поддержка HTML и RTF будет добавлена позже
        if data.html.is_some() {
            debug!("HTML content present but not yet supported for setting");
        }
        if data.rtf.is_some() {
            debug!("RTF content present but not yet supported for setting");
        }

        // Устанавливаем изображение если доступно
        if let Some(ref image_base64) = data.image {
            use base64::{Engine as _, engine::general_purpose};
            match general_purpose::STANDARD.decode(image_base64) {
                Ok(image_bytes) => {
                    // Создаем ImageData для arboard
                    let image_data = arboard::ImageData {
                        width: 0, // TODO: нужно передавать размеры
                        height: 0,
                        bytes: image_bytes.into(),
                    };
                    if let Err(e) = self.arboard.set_image(image_data) {
                        warn!("Failed to set image: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to decode image base64: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Проверить, изменился ли буфер обмена
    pub fn has_clipboard_changed(&mut self, last_content: &str) -> bool {
        match self.arboard.get_text() {
            Ok(current) => current != last_content,
            Err(_) => false,
        }
    }
}
