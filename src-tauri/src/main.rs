// src-tauri/src/main.rs
// src-tauri/src/main.rs
// Prevents additional console window on Windows in release
// 阻止在 Windows 的发布版本中出现额外的控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    scrollery_lib::run()
}
