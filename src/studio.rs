use eframe::egui::{self, Color32, Sense};

use crate::{
    ACCENT, BACKGROUND, MUTED,
    library::{Artwork, DEFAULT_FRAME_MS, Library, MAX_FRAMES},
    protocol::{Command, EDGE, OFF, PIXELS, Pixels, WHITE},
    starter_art,
};

const COLORS: [(u8, &str); 10] = [
    (OFF, "Erase"),
    (WHITE, "White"),
    (0, "Red"),
    (15, "Orange"),
    (30, "Yellow"),
    (60, "Green"),
    (90, "Cyan"),
    (120, "Blue"),
    (140, "Violet"),
    (160, "Pink"),
];

const MOVEMENTS: [&str; 11] = [
    "Still",
    "Up",
    "Down",
    "Left",
    "Right",
    "Blink",
    "Breathe",
    "Loop up",
    "Loop down",
    "Loop left",
    "Loop right",
];

pub enum Action {
    Send(Command),
    Play {
        frames: Vec<Pixels>,
        brightness: u8,
        hold_ms: u16,
    },
    Stop,
}

pub struct Studio {
    frames: Vec<Pixels>,
    editing: usize,
    preview_start: Option<f64>,
    breathe: bool,
    frame_ms: u16,
    brush: u8,
    name: String,
    saved: Vec<Artwork>,
    movement: u8,
    speed: u8,
    brightness: u8,
    status: String,
}

impl Studio {
    pub fn new(library: &Library) -> Self {
        let (saved, status) = match library.artworks() {
            Ok(saved) => (saved, String::new()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (vec![], String::new()),
            Err(error) => (vec![], format!("Could not read saved artwork ({error}).")),
        };
        Self {
            frames: vec![[OFF; PIXELS]],
            editing: 0,
            preview_start: None,
            breathe: false,
            frame_ms: DEFAULT_FRAME_MS,
            brush: 140,
            name: String::new(),
            saved,
            movement: 0,
            speed: 50,
            brightness: 75,
            status,
        }
    }

    pub fn canvas(&mut self, ui: &mut egui::Ui) {
        let side = ui.available_width().min(388.0);
        let (area, response) =
            ui.allocate_exact_size(egui::Vec2::splat(side), Sense::click_and_drag());
        let step = side / EDGE as f32;
        let now = ui.input(|input| input.time);
        let mut glow = 1.0;
        let display = if let Some(start) = self.preview_start {
            let interval = if self.frames.as_slice() == starter_art::marquee() {
                60 + u16::from(100 - self.speed) * 2
            } else {
                self.frame_ms
            };
            let position = (now - start) * 1000.0 / f64::from(interval);
            let index = position.floor() as usize % self.frames.len();
            if self.frames.len() == 2 && self.breathe {
                glow = (std::f64::consts::PI * position.fract()).sin() as f32;
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(25));
            index
        } else {
            if (response.clicked() || response.is_pointer_button_down_on())
                && let Some(position) = response
                    .interact_pointer_pos()
                    .filter(|point| area.contains(*point))
            {
                let cell = |point: egui::Pos2| {
                    let offset = (point - area.min) / step;
                    (offset.x as usize).min(EDGE - 1) + EDGE * (offset.y as usize).min(EDGE - 1)
                };
                if response.dragged() {
                    let delta = ui.input(|input| input.pointer.delta());
                    let samples = ((delta.length() / (step / 2.0)).ceil() as usize).clamp(1, 100);
                    for index in 1..=samples {
                        let point = position - delta * (index as f32 / samples as f32);
                        if area.contains(point) {
                            self.frames[self.editing][cell(point)] = self.brush;
                        }
                    }
                }
                self.frames[self.editing][cell(position)] = self.brush;
            }
            self.editing
        };
        let painter = ui.painter();
        for (index, &pixel) in self.frames[display].iter().enumerate() {
            let offset = egui::vec2((index % EDGE) as f32, (index / EDGE) as f32) * step;
            let rect = egui::Rect::from_min_size(area.min + offset, egui::Vec2::splat(step - 1.0));
            let mut color = preview_color(pixel);
            if self.preview_start.is_some() && pixel != OFF {
                color = color.gamma_multiply(glow * f32::from(self.brightness) / 100.0);
            }
            painter.rect_filled(rect, 2, color);
        }
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("Frame {} of {}", display + 1, self.frames.len()))
                    .color(MUTED),
            );
            if ui
                .add_enabled(
                    self.frames.len() > 1,
                    egui::Button::new(if self.preview_start.is_some() {
                        "Stop preview"
                    } else {
                        "Preview animation"
                    }),
                )
                .clicked()
            {
                self.preview_start = if self.preview_start.is_some() {
                    None
                } else {
                    Some(now)
                };
            }
        });
    }

    pub fn controls(
        &mut self,
        ui: &mut egui::Ui,
        library: &Library,
        connected: bool,
        playing: bool,
    ) -> Option<Action> {
        ui.horizontal_wrapped(|ui| {
            let now = ui.input(|input| input.time);
            if ui.button(starter_art::FLAMINGO).clicked() {
                self.load_starter(starter_art::FLAMINGO, starter_art::flamingo(), 550, 45, now);
            }
            if ui.button("Flamingo text").clicked() {
                self.load_starter(
                    starter_art::MARQUEE,
                    starter_art::marquee().to_vec(),
                    100,
                    55,
                    now,
                );
            }
            if ui
                .button(starter_art::RANDOM)
                .on_hover_text("Give every LED a random hue. Select again to make a new pattern.")
                .clicked()
            {
                self.load_starter(starter_art::RANDOM, starter_art::random_rgb(), 500, 40, now);
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (color, label) in COLORS {
                let button = egui::Button::new("")
                    .min_size(egui::vec2(18.0, 20.0))
                    .fill(preview_color(color))
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        if self.brush == color {
                            Color32::WHITE
                        } else {
                            MUTED
                        },
                    ));
                if ui.add(button).on_hover_text(label).clicked() {
                    self.brush = color;
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Fill").clicked() {
                self.preview_start = None;
                self.frames[self.editing].fill(self.brush);
            }
            if ui.button("Clear").clicked() {
                self.preview_start = None;
                self.frames[self.editing].fill(OFF);
            }
        });
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("edit_frame")
                .selected_text(format!("Frame {}", self.editing + 1))
                .show_ui(ui, |ui| {
                    for index in 0..self.frames.len() {
                        if ui
                            .selectable_value(
                                &mut self.editing,
                                index,
                                format!("Frame {}", index + 1),
                            )
                            .clicked()
                        {
                            self.preview_start = None;
                        }
                    }
                });
            if ui
                .add_enabled(self.frames.len() < MAX_FRAMES, egui::Button::new("Add"))
                .clicked()
            {
                self.insert([OFF; PIXELS]);
            }
            if ui
                .add_enabled(
                    self.frames.len() < MAX_FRAMES,
                    egui::Button::new("Duplicate"),
                )
                .clicked()
            {
                self.insert(self.frames[self.editing]);
            }
            if ui
                .add_enabled(self.frames.len() > 1, egui::Button::new("Remove"))
                .clicked()
            {
                self.frames.remove(self.editing);
                self.editing = self.editing.min(self.frames.len() - 1);
                self.preview_start = None;
            }
        });
        let native_scroll = self.frames.as_slice() == starter_art::marquee();
        match self.frames.len() {
            1 => {
                ui.horizontal(|ui| {
                    ui.label("Movement");
                    egui::ComboBox::from_id_salt("motion")
                        .selected_text(MOVEMENTS[self.movement as usize])
                        .show_ui(ui, |ui| {
                            for (index, label) in MOVEMENTS.iter().enumerate() {
                                ui.selectable_value(&mut self.movement, index as u8, *label);
                            }
                        });
                });
                ui.add(
                    egui::Slider::new(&mut self.speed, 0..=100)
                        .text("Speed")
                        .suffix("%"),
                );
            }
            2 => {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.breathe, false, "Blink");
                    ui.selectable_value(&mut self.breathe, true, "Breathe");
                });
                ui.add(
                    egui::Slider::new(&mut self.speed, 0..=100)
                        .text("Curtain speed")
                        .suffix("%"),
                );
                ui.add(
                    egui::Slider::new(&mut self.frame_ms, 100..=5000)
                        .text("Preview frame")
                        .suffix(" ms"),
                );
            }
            _ if native_scroll => {
                ui.add(
                    egui::Slider::new(&mut self.speed, 0..=100)
                        .text("Scroll speed")
                        .suffix("%"),
                );
                ui.label(egui::RichText::new(starter_art::MESSAGE).color(MUTED));
            }
            _ => {
                ui.add(
                    egui::Slider::new(&mut self.frame_ms, 100..=5000)
                        .text("Frame hold")
                        .suffix(" ms"),
                );
                ui.label(
                    egui::RichText::new(
                        "PC playback adds several seconds per frame for Bluetooth transfer.",
                    )
                    .color(MUTED),
                );
            }
        }
        ui.add(
            egui::Slider::new(&mut self.brightness, 1..=100)
                .text("Brightness")
                .suffix("%"),
        );
        ui.separator();
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.name)
                    .hint_text("Artwork name")
                    .desired_width(160.0)
                    .char_limit(80),
            );
            if ui
                .add_enabled(!self.name.trim().is_empty(), egui::Button::new("Save"))
                .clicked()
            {
                self.save(library);
            }
        });
        egui::ComboBox::from_id_salt("saved_artwork")
            .selected_text("Load saved artwork")
            .show_ui(ui, |ui| {
                for item in &self.saved {
                    if ui
                        .selectable_label(item.name == self.name, &item.name)
                        .clicked()
                    {
                        self.name.clone_from(&item.name);
                        self.frames = std::iter::once(&item.pixels)
                            .chain(&item.extra_frames)
                            .map(|pixels| {
                                <Pixels>::try_from(pixels.as_slice()).expect("validated frame")
                            })
                            .collect();
                        self.editing = 0;
                        self.preview_start = None;
                        self.breathe = item.breathe;
                        self.frame_ms = item.frame_ms;
                        self.movement = item.movement;
                        self.speed = item.speed;
                        self.brightness = item.brightness;
                        self.status = format!("Loaded {}", item.name);
                    }
                }
            });
        if !self.status.is_empty() {
            ui.label(egui::RichText::new(&self.status).color(MUTED));
        }
        if playing {
            return ui
                .button("Stop curtain playback")
                .clicked()
                .then_some(Action::Stop);
        }
        let label = match self.frames.len() {
            1 => "Send artwork",
            2 => "Send animation",
            _ if native_scroll => "Send scrolling text",
            _ => "Play on curtain",
        };
        let send =
            egui::Button::new(egui::RichText::new(label).color(BACKGROUND).strong()).fill(ACCENT);
        if !ui.add_enabled(connected, send).clicked() {
            return None;
        }
        Some(match self.frames.as_slice() {
            [pixels] => Action::Send(Command::Picture {
                pixels: *pixels,
                movement: self.movement,
                speed: self.speed,
                brightness: self.brightness,
            }),
            [first, second] => Action::Send(Command::Animation {
                frames: Box::new([*first, *second]),
                breathe: self.breathe,
                speed: self.speed,
                brightness: self.brightness,
            }),
            _ if native_scroll => Action::Send(Command::TextScroll {
                text: starter_art::MESSAGE,
                speed: self.speed,
                brightness: self.brightness,
            }),
            _ => Action::Play {
                frames: self.frames.clone(),
                brightness: self.brightness,
                hold_ms: self.frame_ms,
            },
        })
    }

    fn load_starter(
        &mut self,
        name: &str,
        frames: Vec<Pixels>,
        frame_ms: u16,
        speed: u8,
        now: f64,
    ) {
        self.name = name.into();
        self.frames = frames;
        self.editing = 0;
        self.preview_start = Some(now);
        self.frame_ms = frame_ms;
        self.speed = speed;
        self.breathe = false;
        self.movement = 0;
        self.brightness = 75;
        self.status.clear();
    }

    fn insert(&mut self, pixels: Pixels) {
        self.editing += 1;
        self.frames.insert(self.editing, pixels);
        self.preview_start = None;
    }

    fn save(&mut self, library: &Library) {
        let name = self.name.trim().to_owned();
        let entry = Artwork {
            name: name.clone(),
            pixels: self.frames[0].to_vec(),
            extra_frames: self.frames[1..]
                .iter()
                .map(|frame| frame.to_vec())
                .collect(),
            breathe: self.breathe,
            frame_ms: self.frame_ms,
            movement: self.movement,
            speed: self.speed,
            brightness: self.brightness,
        };
        let mut revised = self.saved.clone();
        if let Some(previous) = revised.iter_mut().find(|item| item.name == name) {
            *previous = entry;
        } else {
            revised.push(entry);
            revised.sort_by(|left, right| left.name.cmp(&right.name));
        }
        match library.save_artworks(&revised) {
            Ok(()) => {
                self.saved = revised;
                self.name = name;
                self.status = format!("Saved {}", self.name);
            }
            Err(error) => self.status = format!("Could not save artwork ({error})."),
        }
    }
}

fn preview_color(pixel: u8) -> Color32 {
    match pixel {
        OFF => Color32::from_rgb(35, 39, 52),
        WHITE => Color32::WHITE,
        hue => egui::ecolor::Hsva::new(f32::from(hue) / 180.0, 1.0, 1.0, 1.0).into(),
    }
}
