#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod bluetooth;
mod library;
mod protocol;
mod scene_text;
mod scenes;
mod starter_art;
mod studio;
mod ukraine_art;

use eframe::egui::{self, Color32, Context, Frame, RichText, Sense, Stroke, Vec2};
use library::{Library, PRESET_COUNT, Presets};
use protocol::{Command, Lighting, Pixels};
use scenes::Selection;
use studio::{Action as StudioAction, Studio};

const BACKGROUND: Color32 = Color32::from_rgb(15, 17, 23);
const PANEL: Color32 = Color32::from_rgb(24, 27, 37);
const BORDER: Color32 = Color32::from_rgb(47, 51, 65);
const TEXT: Color32 = Color32::from_rgb(241, 243, 251);
const MUTED: Color32 = Color32::from_rgb(167, 174, 195);
const ACCENT: Color32 = Color32::from_rgb(187, 171, 255);
const GREEN: Color32 = Color32::from_rgb(116, 231, 186);

const SWATCHES: [([u8; 3], &str); 7] = [
    ([255, 244, 225], "Warm white"),
    ([255, 74, 87], "Coral"),
    ([255, 166, 63], "Amber"),
    ([112, 255, 162], "Mint"),
    ([56, 214, 255], "Sky blue"),
    ([165, 124, 255], "Lavender"),
    ([255, 125, 198], "Pink"),
];

enum Request {
    Connect(String),
    Disconnect,
    Send(Command),
}

#[derive(Default)]
struct Snapshot {
    presets: Option<Presets>,
    power: Option<bool>,
    music: bool,
    picture: bool,
}

enum Event {
    Connected(Snapshot),
    Disconnected(String),
    Sent(Command),
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Solid,
    Scene,
    Music,
    Studio,
}

struct CurtainPlayback {
    frames: Vec<Pixels>,
    next_frame: usize,
    brightness: u8,
    hold: std::time::Duration,
    next_send: Option<std::time::Instant>,
}

struct App {
    demo: bool,
    address: String,
    connected: bool,
    busy: bool,
    status: String,
    power: Option<bool>,
    mode: Mode,
    rgb: [u8; 3],
    brightness: u8,
    scene: Selection,
    scene_speed: u8,
    scene_speed_available: bool,
    scene_tiles: scenes::Tiles,
    presets: Presets,
    library: Library,
    music_available: bool,
    music_mode: u8,
    sensitivity: u8,
    automatic: bool,
    picture_available: bool,
    studio: Studio,
    curtain_playback: Option<CurtainPlayback>,
    #[cfg(windows)]
    bluetooth: Option<(
        tokio::sync::mpsc::UnboundedSender<Request>,
        std::sync::mpsc::Receiver<Event>,
    )>,
}

impl App {
    fn new(context: &Context, demo: bool) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(TEXT);
        visuals.panel_fill = BACKGROUND;
        visuals.selection.bg_fill = Color32::from_rgb(77, 59, 135);
        visuals.slider_trailing_fill = true;
        context.set_visuals(visuals);
        context.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.spacing.button_padding = egui::vec2(12.0, 7.0);
        });

        #[cfg(windows)]
        let bluetooth = (!demo).then(|| {
            let (sender, receiver) = std::sync::mpsc::channel();
            (bluetooth::start(context.clone(), sender), receiver)
        });

        let library = Library::new(demo);
        let mut presets = library.presets().unwrap_or_else(|_| Presets {
            ids: (1..=PRESET_COUNT).collect(),
            ..Presets::default()
        });
        scenes::extend_ids(&mut presets.ids);
        let scene = Selection::Firmware(presets.last_scene.unwrap_or(1));
        let studio = Studio::new(&library);

        Self {
            demo,
            address: protocol::configured_address(),
            connected: false,
            busy: false,
            status: if demo {
                "Preview mode".into()
            } else {
                "Ready to connect".into()
            },
            power: None,
            mode: Mode::Solid,
            rgb: [165, 124, 255],
            brightness: 75,
            scene,
            scene_speed: presets.scene_speed.unwrap_or(75),
            scene_speed_available: demo,
            scene_tiles: scenes::Tiles::default(),
            presets,
            library,
            music_available: false,
            music_mode: 1,
            sensitivity: 65,
            automatic: true,
            picture_available: false,
            studio,
            curtain_playback: None,
            #[cfg(windows)]
            bluetooth,
        }
    }

    fn request(&mut self, request: Request) {
        if let Request::Connect(address) = &request
            && !self.demo
            && address.is_empty()
        {
            self.status = "Enter the curtain’s Bluetooth address.".into();
            return;
        }
        self.busy = true;
        self.status = match request {
            Request::Connect(_) => "Looking for your curtain…",
            Request::Disconnect => "Disconnecting…",
            Request::Send(_) => "Sending to curtain…",
        }
        .into();

        if self.demo {
            match request {
                Request::Connect(_) => self.handle(Event::Connected(Snapshot {
                    music: true,
                    picture: true,
                    ..Snapshot::default()
                })),
                Request::Disconnect => self.handle(Event::Disconnected("Preview mode".into())),
                Request::Send(command) => {
                    // Exercise the same packet encoders used by the Bluetooth transport.
                    match &command {
                        Command::Power(on) => protocol::power(*on),
                        Command::Apply {
                            lighting,
                            brightness,
                            ..
                        } => protocol::lighting(*lighting, *brightness),
                        Command::Picture {
                            pixels,
                            movement,
                            speed,
                            brightness,
                        } => {
                            let mut packets = protocol::picture_chunks(pixels).unwrap();
                            packets.push(protocol::picture_settings(
                                *movement,
                                *brightness,
                                *speed,
                                1,
                            ));
                            packets.concat()
                        }
                        Command::TextScroll {
                            text,
                            speed,
                            brightness,
                        } => protocol::scroll_text_packets(text, *speed, *brightness)
                            .unwrap()
                            .concat(),
                        Command::Animation {
                            frames,
                            breathe,
                            speed,
                            brightness,
                        } => protocol::animation_packets(frames, *breathe, *speed, *brightness)
                            .unwrap()
                            .into_iter()
                            .flat_map(|(packet, _)| packet)
                            .collect(),
                    };
                    self.handle(Event::Sent(command));
                }
            }
            return;
        }

        #[cfg(windows)]
        if let Some((sender, _)) = &self.bluetooth
            && sender.send(request).is_ok()
        {
            return;
        }
        self.handle(Event::Disconnected(
            "Bluetooth is unavailable in this session.".into(),
        ));
    }

    fn handle(&mut self, event: Event) {
        self.busy = false;
        match event {
            Event::Connected(snapshot) => {
                self.connected = true;
                self.power = snapshot.power;
                self.music_available = snapshot.music;
                self.picture_available = snapshot.picture;
                self.scene_speed_available = self.demo
                    || snapshot
                        .presets
                        .as_ref()
                        .is_some_and(|presets| presets.scene_speed.is_some());
                if let Some(speed) = snapshot
                    .presets
                    .as_ref()
                    .and_then(|presets| presets.scene_speed)
                {
                    self.scene_speed = speed;
                }
                self.status = if self.demo {
                    "Preview connected"
                } else {
                    "Connected"
                }
                .into();
                if let Some(mut presets) = snapshot.presets {
                    if let Some(scene) = presets.last_scene
                        && let Selection::Firmware(_) = self.scene
                    {
                        self.scene = Selection::Firmware(scene);
                    }
                    if self.presets.model == presets.model
                        && self.presets.firmware == presets.firmware
                    {
                        presets.last_scene = presets.last_scene.or(self.presets.last_scene);
                        presets.scene_speed = presets.scene_speed.or(self.presets.scene_speed);
                        presets.ids.extend_from_slice(&self.presets.ids);
                        presets.ids.sort_unstable();
                        presets.ids.dedup();
                    }
                    scenes::extend_ids(&mut presets.ids);
                    if let Selection::Firmware(id) = self.scene
                        && !presets.ids.contains(&id)
                    {
                        self.scene = Selection::Firmware(presets.last_scene.unwrap_or(1));
                    }
                    self.presets = presets;
                    self.cache_presets();
                }
            }
            Event::Disconnected(message) => {
                self.connected = false;
                self.curtain_playback = None;
                self.power = None;
                self.status = message;
            }
            Event::Sent(command) => {
                self.power = Some(!matches!(command, Command::Power(false)));
                self.status = if self.demo {
                    "Preview updated"
                } else if matches!(command, Command::Animation { .. }) {
                    "Curtain acknowledged animation"
                } else if matches!(command, Command::TextScroll { .. }) {
                    "Curtain started scrolling text"
                } else if matches!(command, Command::Picture { .. }) {
                    "Picture sent to curtain"
                } else {
                    "Curtain acknowledged command"
                }
                .into();
                if let Some(playback) = &mut self.curtain_playback
                    && matches!(command, Command::Picture { .. })
                {
                    playback.next_send = Some(std::time::Instant::now() + playback.hold);
                    let frame =
                        (playback.next_frame + playback.frames.len() - 1) % playback.frames.len();
                    self.status = format!(
                        "Curtain playback · frame {} of {}",
                        frame + 1,
                        playback.frames.len()
                    );
                }
                if let Command::Apply {
                    lighting: Lighting::Scene(scene),
                    scene_speed,
                    ..
                } = command
                    && !self.presets.model.is_empty()
                {
                    self.presets.last_scene = Some(scene);
                    self.presets.scene_speed = scene_speed.or(self.presets.scene_speed);
                    self.cache_presets();
                }
            }
        }
    }

    fn studio_action(&mut self, action: StudioAction) {
        match action {
            StudioAction::Send(command) => self.request(Request::Send(command)),
            StudioAction::Play {
                frames,
                brightness,
                hold_ms,
            } => {
                self.curtain_playback = Some(CurtainPlayback {
                    frames,
                    brightness,
                    next_frame: 0,
                    hold: std::time::Duration::from_millis(u64::from(hold_ms)),
                    next_send: Some(std::time::Instant::now()),
                });
            }
            StudioAction::Stop => {
                self.curtain_playback = None;
                self.status = if self.busy {
                    "Stopping after this frame"
                } else {
                    "Curtain playback stopped"
                }
                .into();
            }
        }
    }

    fn advance_playback(&mut self, context: &Context) {
        if self.busy || !self.connected {
            return;
        }
        let Some(playback) = &mut self.curtain_playback else {
            return;
        };
        let Some(next_send) = playback.next_send else {
            return;
        };
        let wait = next_send.saturating_duration_since(std::time::Instant::now());
        if !wait.is_zero() {
            context.request_repaint_after(wait);
            return;
        }
        let command = Command::Picture {
            pixels: playback.frames[playback.next_frame],
            movement: 0,
            speed: 100,
            brightness: playback.brightness,
        };
        playback.next_frame = (playback.next_frame + 1) % playback.frames.len();
        playback.next_send = None;
        self.request(Request::Send(command));
        if self.demo {
            context.request_repaint();
        }
    }

    fn cache_presets(&mut self) {
        if let Err(error) = self.library.save_presets(&self.presets) {
            self.status = format!("Connected. Could not save the preset cache ({error}).");
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Hypelights").size(32.0).strong());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("BESL18P").color(MUTED));
                    if self.connected || self.demo {
                        if !self.address.is_empty() {
                            ui.label(RichText::new(&self.address).color(MUTED));
                        }
                    } else {
                        ui.add_enabled(
                            !self.busy,
                            egui::TextEdit::singleline(&mut self.address)
                                .desired_width(150.0)
                                .char_limit(30)
                                .hint_text("Bluetooth address"),
                        );
                    }
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if self.connected {
                    "Disconnect"
                } else {
                    "Connect"
                };
                if ui
                    .add_enabled(
                        !self.busy
                            && (self.connected || self.demo || !self.address.trim().is_empty()),
                        egui::Button::new(label),
                    )
                    .clicked()
                {
                    let request = if self.connected {
                        Request::Disconnect
                    } else {
                        Request::Connect(self.address.trim().to_owned())
                    };
                    self.request(request);
                }
                if self.demo {
                    ui.label(RichText::new("Demo").color(ACCENT));
                }
            });
        });
    }

    fn preview(&self, ui: &mut egui::Ui) {
        let side = ui.available_width().min(388.0);
        let (area, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
        let paint = ui.painter();
        paint.rect_filled(area, 8, Color32::from_rgb(19, 21, 30));
        let canvas = area.shrink2(egui::vec2(17.0, 20.0));
        let preview = if self.mode == Mode::Scene {
            self.scene.frames()
        } else {
            None
        };
        let scene_frame = preview.map(|frames| {
            let duration = self.scene.preview_frame_seconds(self.scene_speed);
            let now = ui.input(|input| input.time);
            if frames.len() > 1 {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(
                        duration - now % duration,
                    ));
            }
            &frames[(now / duration) as usize % frames.len()]
        });
        let [red, green, blue] = self.rgb;
        let color = Color32::from_rgb(red, green, blue);
        for column in 0..20 {
            let x = egui::lerp(canvas.x_range(), column as f32 / 19.0);
            paint.line_segment(
                [egui::pos2(x, canvas.top()), egui::pos2(x, canvas.bottom())],
                Stroke::new(1.0_f32, BORDER),
            );
            for row in 0..20 {
                let y = egui::lerp(canvas.y_range(), row as f32 / 19.0);
                let center = egui::pos2(x, y);
                let lit = match self.mode {
                    Mode::Solid => color.gamma_multiply(f32::from(self.brightness) / 100.0),
                    Mode::Scene => scene_frame.map_or(Color32::from_rgb(49, 53, 68), |frame| {
                        let color = frame[row * protocol::EDGE + column];
                        if color == Color32::BLACK {
                            Color32::from_rgb(33, 36, 48)
                        } else {
                            color.gamma_multiply(f32::from(self.brightness) / 100.0)
                        }
                    }),
                    Mode::Music | Mode::Studio => Color32::from_rgb(66, 70, 86),
                };
                paint.circle_filled(center, 4.2, lit.gamma_multiply(0.21));
                paint.circle_filled(center, 1.9, lit);
            }
        }
        let label = match self.mode {
            Mode::Solid => "Color preview",
            Mode::Scene if matches!(self.scene, Selection::Firmware(id) if scene_text::contains(id)) => {
                "Scrolling text preview"
            }
            Mode::Scene if self.scene.is_artwork() => "Scene preview",
            Mode::Scene if preview.is_some() => "Vendor scene preview",
            Mode::Scene => "Preview this preset on the curtain",
            Mode::Music => "The curtain responds to its microphone",
            Mode::Studio => "",
        };
        ui.label(RichText::new(label).color(MUTED));
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        let ready = self.connected && !self.busy && self.curtain_playback.is_none();
        ui.label(RichText::new("Power").size(20.0).strong());
        ui.horizontal(|ui| {
            for (on, label) in [(true, "Turn on"), (false, "Turn off")] {
                let button = egui::Button::new(label).selected(self.power == Some(on));
                if ui.add_enabled(ready, button).clicked() {
                    self.request(Request::Send(Command::Power(on)));
                }
            }
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(RichText::new("Lighting").size(20.0).strong());
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.mode, Mode::Solid, "Solid color");
            ui.selectable_value(&mut self.mode, Mode::Scene, "Built-in scenes");
            ui.selectable_value(&mut self.mode, Mode::Music, "Music");
            ui.selectable_value(&mut self.mode, Mode::Studio, "Studio");
        });
        ui.add_space(5.0);
        match self.mode {
            Mode::Solid => {
                ui.horizontal(|ui| {
                    for (rgb, name) in SWATCHES {
                        let swatch = egui::Button::new("")
                            .min_size(egui::vec2(23.0, 27.0))
                            .fill(Color32::from_rgb(rgb[0], rgb[1], rgb[2]))
                            .stroke(Stroke::new(
                                1.0_f32,
                                if rgb == self.rgb { TEXT } else { BORDER },
                            ));
                        if ui.add(swatch).on_hover_text(name).clicked() {
                            self.rgb = rgb;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.color_edit_button_srgb(&mut self.rgb);
                    ui.label(
                        RichText::new(format!(
                            "#{:02X}{:02X}{:02X}",
                            self.rgb[0], self.rgb[1], self.rgb[2]
                        ))
                        .monospace(),
                    );
                });
            }
            Mode::Scene => {
                let name = self.scene.name();
                let title = self
                    .scene
                    .category()
                    .filter(|_| !self.scene.is_artwork())
                    .map(|category| format!("{name} · {category}"))
                    .unwrap_or_else(|| name.into_owned());
                ui.label(RichText::new(title).strong());
                ui.add(
                    egui::Slider::new(&mut self.scene_speed, 0..=100)
                        .text("Scene speed")
                        .suffix("%"),
                )
                .on_hover_text(
                    "Changes preview speed. Apply lighting sends the speed to supported curtains.",
                );
            }
            Mode::Music => {
                ui.horizontal(|ui| {
                    ui.label("Microphone effect");
                    ui.add(egui::DragValue::new(&mut self.music_mode).range(1..=4));
                });
                ui.add(
                    egui::Slider::new(&mut self.sensitivity, 1..=100)
                        .text("Sensitivity")
                        .suffix("%"),
                );
                ui.add_enabled(
                    self.music_mode != 1,
                    egui::Checkbox::new(&mut self.automatic, "Automatic colors"),
                );
                if self.music_mode != 1 && !self.automatic {
                    ui.color_edit_button_srgb(&mut self.rgb);
                }
            }
            Mode::Studio => {
                let ready = ready && self.picture_available;
                if let Some(action) =
                    self.studio
                        .controls(ui, &self.library, ready, self.curtain_playback.is_some())
                {
                    self.studio_action(action);
                }
                if self.connected && !self.picture_available {
                    ui.label(
                        RichText::new(
                            "The current Bluetooth connection cannot transfer custom pictures.",
                        )
                        .color(MUTED),
                    );
                }
                return;
            }
        }
        ui.add_space(10.0);
        ui.label("Brightness");
        ui.add(egui::Slider::new(&mut self.brightness, 1..=100).suffix("%"));
        ui.add_space(9.0);
        let apply = egui::Button::new(RichText::new("Apply lighting").color(BACKGROUND).strong())
            .fill(ACCENT);
        let ready = ready
            && match self.mode {
                Mode::Music => self.music_available,
                Mode::Scene if self.scene.is_artwork() => self.picture_available,
                _ => true,
            };
        if ui.add_enabled(ready, apply).clicked() {
            let apply = |lighting| Command::Apply {
                lighting,
                brightness: self.brightness,
                scene_speed: None,
            };
            let command = match self.mode {
                Mode::Solid => apply(Lighting::Solid(self.rgb)),
                Mode::Scene => self.scene.command(
                    self.brightness,
                    (self.scene.is_artwork() || self.scene_speed_available)
                        .then_some(self.scene_speed),
                ),
                Mode::Music => apply(Lighting::Music {
                    mode: self.music_mode,
                    sensitivity: self.sensitivity,
                    rgb: self.rgb,
                    automatic: self.automatic,
                }),
                Mode::Studio => return,
            };
            self.request(Request::Send(command));
        }
        if self.mode == Mode::Scene {
            ui.add_space(9.0);
            self.scene_tiles
                .show(ui, &self.presets.ids, &mut self.scene, self.scene_speed);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, context: &Context, _: &mut eframe::Frame) {
        #[cfg(windows)]
        if let Some((_, receiver)) = &self.bluetooth {
            let events: Vec<_> = receiver.try_iter().collect();
            for event in events {
                self.handle(event);
            }
        }
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(BACKGROUND).inner_margin(25))
            .show(context, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.header(ui);
                    ui.add_space(14.0);
                    Frame::new()
                        .fill(PANEL)
                        .stroke(Stroke::new(1.0_f32, BORDER))
                        .corner_radius(10)
                        .inner_margin(16)
                        .show(ui, |ui| {
                            ui.columns(2, |columns| {
                                if self.mode == Mode::Studio {
                                    self.studio.canvas(&mut columns[0]);
                                } else {
                                    self.preview(&mut columns[0]);
                                }
                                self.controls(&mut columns[1]);
                            });
                        });
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        if self.busy {
                            ui.spinner();
                        } else {
                            ui.label(RichText::new("●").color(if self.connected {
                                GREEN
                            } else {
                                MUTED
                            }));
                        }
                        ui.label(RichText::new(&self.status).color(MUTED));
                        if self.curtain_playback.is_some()
                            && self.mode != Mode::Studio
                            && ui.button("Stop curtain playback").clicked()
                        {
                            self.studio_action(StudioAction::Stop);
                        }
                    });
                });
            });
        self.advance_playback(context);
    }
}

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let demo = args.iter().any(|argument| argument == "--demo");
    let connect = args.iter().any(|argument| argument == "--connect");
    if !cfg!(windows) && !demo {
        eprintln!(
            "Run Hypelights on Windows for Bluetooth control, or pass --demo to preview the app."
        );
        return Ok(());
    }
    eframe::run_native(
        "Hypelights",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([920.0, 740.0])
                .with_min_inner_size([770.0, 660.0]),
            ..Default::default()
        },
        Box::new(move |creation| {
            let mut app = App::new(&creation.egui_ctx, demo);
            if connect && (demo || !app.address.trim().is_empty()) {
                app.request(Request::Connect(app.address.trim().to_owned()));
            }
            Ok(Box::new(app))
        }),
    )
}
