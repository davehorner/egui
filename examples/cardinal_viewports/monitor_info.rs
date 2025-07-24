// --- Monitor info using display-info crate ---
use display_info::DisplayInfo;
use egui::{Pos2, Rect};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static MONITOR_RECTS: Lazy<Mutex<Vec<Rect>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub fn fill_monitor_rects() {
    let mut rects = Vec::new();
    for display in DisplayInfo::all().unwrap_or_default() {
        let min = Pos2::new(display.x as f32, display.y as f32);
        let max = Pos2::new(
            (display.x + display.width as i32) as f32,
            (display.y + display.height as i32) as f32,
        );
        rects.push(Rect::from_min_max(min, max));
    }
    *MONITOR_RECTS.lock().unwrap() = rects;
}
