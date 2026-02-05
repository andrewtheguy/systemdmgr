use ratatui::style::Color;
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, Deserialize)]
pub struct SystemdService {
    pub unit: String,
    #[allow(dead_code)]
    pub load: String,
    #[allow(dead_code)]
    pub active: String,
    pub sub: String,
    pub description: String,
}

impl SystemdService {
    pub fn status_display(&self) -> &str {
        &self.sub
    }

    pub fn status_color(&self) -> Color {
        match self.sub.as_str() {
            "running" => Color::Green,
            "exited" => Color::Yellow,
            "dead" | "stopped" => Color::DarkGray,
            "failed" => Color::Red,
            _ => Color::White,
        }
    }
}

pub fn fetch_logs(unit_name: &str, lines: usize) -> Result<Vec<String>, String> {
    let output = Command::new("journalctl")
        .args(["-u", unit_name, "-n", &lines.to_string(), "--no-pager"])
        .output()
        .map_err(|e| format!("Failed to execute journalctl: {}", e))?;

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect())
}

pub fn fetch_services() -> Result<Vec<SystemdService>, String> {
    let output = Command::new("systemctl")
        .args(["list-units", "--type=service", "--all", "--no-pager", "--output=json"])
        .output()
        .map_err(|e| format!("Failed to execute systemctl: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "systemctl failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let services: Vec<SystemdService> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    Ok(services)
}
