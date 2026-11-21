//! The build's own facts, for the About page: when it was made, and by
//! whom.

use std::process::Command;

fn main() {
    let by = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    let time = Command::new("date")
        .arg("+%Y-%m-%d %H:%M")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=SKYWARD_BUILD_BY={by}");
    println!("cargo:rustc-env=SKYWARD_BUILD_TIME={time}");
}
