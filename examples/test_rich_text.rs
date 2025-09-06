use arboard::Clipboard;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut clipboard = Clipboard::new()?;
    
    println!("=== Arboard Rich Text Investigation ===");
    
    // 1. Получаем текущий текст из буфера обмена
    match clipboard.get_text() {
        Ok(text) => {
            println!("Current clipboard text: {:?}", text);
            println!("Text length: {} characters", text.len());
        }
        Err(e) => println!("Error getting text: {}", e),
    }
    
    // 2. Попробуем установить HTML-подобный текст
    let rich_text = "This is **bold** and *italic* text with a link: https://example.com";
    clipboard.set_text(rich_text)?;
    println!("Set rich-like text: {}", rich_text);
    
    // 3. Получим обратно и посмотрим что получилось
    match clipboard.get_text() {
        Ok(text) => {
            println!("Retrieved text: {:?}", text);
            println!("Same as original: {}", text == rich_text);
        }
        Err(e) => println!("Error getting text back: {}", e),
    }
    
    // 4. Проверим поддержку изображений
    println!("\n=== Image Support Check ===");
    match clipboard.get_image() {
        Ok(image) => {
            println!("Found image in clipboard: {}x{}", image.width, image.height);
        }
        Err(e) => println!("No image or error: {}", e),
    }
    
    Ok(())
}
