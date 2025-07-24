//! Example: Cardinal Viewports
// Spawns animated viewport windows in N, S, W, E directions on key press.

use eframe::egui;
use egui::{ViewportBuilder, ViewportId};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Direction {
    North,
    South,
    West,
    East,
}

impl Direction {
    fn from_key(key: egui::Key) -> Option<Self> {
        match key {
            egui::Key::N => Some(Direction::North),
            egui::Key::S => Some(Direction::South),
            egui::Key::W => Some(Direction::West),
            egui::Key::E => Some(Direction::East),
            _ => None,
        }
    }

    fn vector(&self) -> (f32, f32) {
        match self {
            Direction::North => (0.0, -1.0), // move up (away from center)
            Direction::South => (0.0, 1.0),  // move down (away from center)
            Direction::West => (-1.0, 0.0),  // move left (away from center)
            Direction::East => (1.0, 0.0),   // move right (away from center)
        }
    }
}

struct CardinalViewport {
    direction: Direction,
    position: egui::Pos2,
    velocity: f32,
    open: bool,
}

pub struct CardinalViewportsApp {
    viewports: Vec<CardinalViewport>,
}

impl Default for CardinalViewportsApp {
    fn default() -> Self {
        Self {
            viewports: Vec::new(),
        }
    }
}

impl eframe::App for CardinalViewportsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Listen for key presses and button clicks
        let mut spawn_direction: Option<Direction> = None;
        egui::TopBottomPanel::top("cardinal_controls_top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Cardinal Viewports Example");
                ui.label("Press N, S, W, E or use the buttons below to spawn animated viewports in each direction.");
            });
            ui.horizontal(|ui| {
                if ui.button("North").clicked() {
                    spawn_direction = Some(Direction::North);
                }
                if ui.button("South").clicked() {
                    spawn_direction = Some(Direction::South);
                }
                if ui.button("West").clicked() {
                    spawn_direction = Some(Direction::West);
                }
                if ui.button("East").clicked() {
                    spawn_direction = Some(Direction::East);
                }
            });
        });

        for key in [egui::Key::N, egui::Key::S, egui::Key::W, egui::Key::E] {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                spawn_direction = Direction::from_key(key);
            }
        }

        if let Some(direction) = spawn_direction {
            if let (Some(parent_rect), Some(monitor_size)) = (
                ctx.input(|i| i.viewport().outer_rect),
                ctx.input(|i| i.viewport().monitor_size),
            ) {
                let offset = 100.0;
                let monitor_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, monitor_size);
                let start_pos = match direction {
                    Direction::North => {
                        egui::pos2(parent_rect.center().x, parent_rect.top() - offset)
                    } // above window
                    Direction::South => {
                        egui::pos2(parent_rect.center().x, parent_rect.bottom() + offset)
                    } // below window
                    Direction::West => {
                        egui::pos2(parent_rect.left() - offset, parent_rect.center().y)
                    } // left of window
                    Direction::East => {
                        egui::pos2(parent_rect.right() + offset, parent_rect.center().y)
                    } // right of window
                };
                self.viewports.push(CardinalViewport {
                    direction,
                    position: start_pos,
                    velocity: 8.0,
                    open: true,
                });
            }
        }

        if let (Some(parent_rect), Some(monitor_size)) = (
            ctx.input(|i| i.viewport().outer_rect),
            ctx.input(|i| i.viewport().monitor_size),
        ) {
            let monitor_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, monitor_size);
            for (i, viewport) in self.viewports.iter_mut().enumerate() {
                if !viewport.open {
                    continue;
                }
                let (dx, dy) = viewport.direction.vector();
                viewport.position.x += dx * viewport.velocity;
                viewport.position.y += dy * viewport.velocity;

                // Wrap around monitor bounds
                if viewport.position.x < monitor_rect.left() {
                    viewport.position.x = monitor_rect.right();
                }
                if viewport.position.x > monitor_rect.right() {
                    viewport.position.x = monitor_rect.left();
                }
                if viewport.position.y < monitor_rect.top() {
                    viewport.position.y = monitor_rect.bottom();
                }
                if viewport.position.y > monitor_rect.bottom() {
                    viewport.position.y = monitor_rect.top();
                }

                // Collision with parent window
                let parent_center = parent_rect.center();
                let dist = ((viewport.position.x - parent_center.x).powi(2)
                    + (viewport.position.y - parent_center.y).powi(2))
                .sqrt();
                if dist < 100.0 {
                    // Beep and close
                    #[cfg(target_os = "windows")]
                    {
                        unsafe { winapi::um::winuser::MessageBeep(0xFFFFFFFF) };
                    }
                    #[cfg(target_os = "linux")]
                    println!("\x07");
                    #[cfg(target_os = "macos")]
                    println!("\x07");
                    viewport.open = false;
                }

                let viewport_id = ViewportId::from_hash_of(format!("cardinal_{i}"));
                ctx.show_viewport_immediate(
                    viewport_id,
                    ViewportBuilder::default()
                        .with_title(format!("Viewport: {:?}", viewport.direction))
                        .with_inner_size([200.0, 100.0])
                        .with_position(viewport.position),
                    move |ctx, class| {
                        if class == egui::ViewportClass::Embedded {
                            egui::Window::new("Cardinal Viewport").show(ctx, |ui| {
                                ui.label(
                                    "This egui integration does not support multiple viewports",
                                );
                            });
                        } else {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                ui.label(format!("Moving {:?}", viewport.direction));
                            });
                        }
                    },
                );
            }
        }

        // ...existing code...
    }
}
fn main() {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Cardinal Viewports",
        options,
        Box::new(|_cc| Ok(Box::new(CardinalViewportsApp::default()))),
    );
}
