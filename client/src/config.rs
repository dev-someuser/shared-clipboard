use std::fs;
use std::path::PathBuf;

fn config_path() -> Option<PathBuf> {
    let dir = dirs::config_dir()?;
    let path = dir.join("shared-clipboard");
    let _ = fs::create_dir_all(&path);
    Some(path.join("config.toml"))
}

pub fn load_server_url() -> Option<String> {
    let path = config_path()?;
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("server_url=") {
            return Some(rest.trim_matches('"').to_string());
        }
    }
    None
}

pub fn save_server_url(url: &str) -> std::io::Result<()> {
    if let Some(path) = config_path() {
        let content = format!("server_url=\"{}\"\n", url);
        fs::write(path, content)?;
    }
    Ok(())
}

