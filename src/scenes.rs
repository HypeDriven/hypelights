use std::sync::OnceLock;

use eframe::egui::Color32;
use serde::Deserialize;

use crate::{
    protocol::{Command, Lighting, PIXELS, Pixels},
    scene_text, ukraine_art,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Selection {
    UkraineFlag,
    UkraineTrident,
    Firmware(u8),
}

impl Selection {
    pub fn name(self) -> std::borrow::Cow<'static, str> {
        match self {
            Self::UkraineFlag => "Ukrainian Flag".into(),
            Self::UkraineTrident => "Ukrainian Trident".into(),
            Self::Firmware(id) => preview(id).map_or_else(
                || format!("Scene {id}").into(),
                |scene| scene.name.as_str().into(),
            ),
        }
    }

    pub fn category(self) -> Option<&'static str> {
        match self {
            Self::UkraineFlag | Self::UkraineTrident => Some("Ukraine"),
            Self::Firmware(id) => preview(id).map(|scene| scene.category.as_str()),
        }
    }

    pub fn is_artwork(self) -> bool {
        !matches!(self, Self::Firmware(_))
    }

    pub fn frames(self) -> Option<&'static [Vec<Color32>]> {
        match self {
            Self::Firmware(id) => preview(id)
                .filter(|scene| !scene.frames.is_empty())
                .map(|scene| scene.frames.as_slice()),
            artwork => {
                static FLAG: OnceLock<[Vec<Color32>; 2]> = OnceLock::new();
                static TRIDENT: OnceLock<[Vec<Color32>; 2]> = OnceLock::new();
                let entry = if artwork == Self::UkraineFlag {
                    &FLAG
                } else {
                    &TRIDENT
                };
                Some(entry.get_or_init(|| {
                    artwork.artwork_frames().map(|frame| {
                        frame
                            .into_iter()
                            .map(|pixel| match pixel {
                                ukraine_art::BLUE => Color32::from_rgb(0, 87, 183),
                                ukraine_art::YELLOW => Color32::from_rgb(255, 215, 0),
                                ukraine_art::AMBER => Color32::from_rgb(255, 162, 0),
                                crate::protocol::WHITE => Color32::WHITE,
                                _ => Color32::BLACK,
                            })
                            .collect()
                    })
                }))
            }
        }
    }

    pub fn preview_frame_seconds(self, speed: u8) -> f64 {
        let delay = f64::from(100 - speed.min(100));
        if matches!(self, Self::Firmware(id) if scene_text::contains(id)) {
            0.045 + 0.002 * delay
        } else {
            0.12 + 0.007 * delay
        }
    }

    pub fn command(self, brightness: u8, scene_speed: Option<u8>) -> Command {
        match self {
            Self::Firmware(id) => Command::Apply {
                lighting: Lighting::Scene(id),
                brightness,
                scene_speed,
            },
            artwork => Command::Animation {
                frames: Box::new(artwork.artwork_frames()),
                breathe: false,
                speed: scene_speed.unwrap_or(50),
                brightness,
            },
        }
    }

    fn artwork_frames(self) -> [Pixels; 2] {
        match self {
            Self::UkraineFlag => ukraine_art::flag(),
            Self::UkraineTrident => ukraine_art::trident(),
            Self::Firmware(_) => unreachable!("firmware presets use a scene command"),
        }
    }
}

#[derive(Deserialize)]
struct PreviewSource {
    id: u8,
    category: String,
    name: String,
    #[serde(default)]
    frames: Vec<String>,
}

pub struct ScenePreview {
    pub id: u8,
    pub category: String,
    pub name: String,
    pub frames: Vec<Vec<Color32>>,
}

pub fn catalog() -> &'static [ScenePreview] {
    static CATALOG: OnceLock<Vec<ScenePreview>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        #[cfg(hypelights_vendor_previews)]
        const CATALOG_DATA: &str = include_str!("../assets/scene-previews.json");
        #[cfg(not(hypelights_vendor_previews))]
        const CATALOG_DATA: &str = include_str!("../assets/scene-catalog.json");
        let sources: Vec<PreviewSource> =
            serde_json::from_str(CATALOG_DATA).expect("embedded scene catalog");
        sources
            .into_iter()
            .map(|source| ScenePreview {
                id: source.id,
                category: source.category,
                name: source.name,
                frames: source
                    .frames
                    .iter()
                    .map(|frame| decode_frame(frame))
                    .collect(),
            })
            .chain(scene_text::previews())
            .collect()
    })
}

pub fn preview(id: u8) -> Option<&'static ScenePreview> {
    catalog().iter().find(|scene| scene.id == id)
}

pub fn extend_ids(ids: &mut Vec<u8>) {
    ids.extend(catalog().iter().map(|scene| scene.id));
    ids.sort_unstable();
    ids.dedup();
}

fn decode_frame(encoded: &str) -> Vec<Color32> {
    assert_eq!(encoded.len(), PIXELS * 6, "embedded 20 × 20 scene frame");
    (0..PIXELS)
        .map(|index| {
            let offset = index * 6;
            let channel = |start| {
                u8::from_str_radix(&encoded[start..start + 2], 16).expect("embedded scene color")
            };
            Color32::from_rgb(channel(offset), channel(offset + 2), channel(offset + 4))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_scene_previews_keep_their_firmware_ids() {
        for (id, title) in (58..=63).zip([
            "Hello Fairy",
            "Welcome",
            "Merry Christmas",
            "Happy New Year",
            "Good Luck",
            "Happy Birthday",
        ]) {
            let scene = preview(id).expect("scrolling preview");
            assert_eq!(scene.name, title);
            assert_ne!(scene.frames[0], scene.frames[1]);
            assert!(matches!(
                Selection::Firmware(id).command(75, None),
                Command::Apply { lighting: Lighting::Scene(selected), .. } if selected == id
            ));
        }
    }

    #[test]
    fn catalog_contains_verified_20_by_20_previews() {
        let entries = catalog();
        let mut ids = Vec::new();
        extend_ids(&mut ids);
        assert_eq!(ids.len(), entries.len());
        assert_eq!(preview(2).unwrap().name, "Snowflake");
        assert_eq!(preview(7).unwrap().category, "Halloween");
        assert!(entries.iter().any(|scene| scene.frames.len() > 1));
        assert!(
            entries
                .iter()
                .flat_map(|scene| &scene.frames)
                .all(|frame| frame.len() == PIXELS)
        );
        assert_eq!(
            Selection::Firmware(2).frames().is_some(),
            cfg!(hypelights_vendor_previews)
        );
    }
}

#[derive(Default)]
pub struct Tiles {
    textures: std::collections::HashMap<(Selection, usize), eframe::egui::TextureHandle>,
}

impl Tiles {
    pub fn show(
        &mut self,
        ui: &mut eframe::egui::Ui,
        ids: &[u8],
        selected: &mut Selection,
        speed: u8,
    ) {
        use eframe::egui::{self, Button, FontId, Rect, TextureOptions, pos2, vec2};
        const HEIGHT: f32 = 96.0;
        let gap = ui.spacing().item_spacing.x;
        let space = (ui.available_width() - 14.0).max(80.0);
        let columns = ((space + gap) / (88.0 + gap)).floor().max(1.0) as usize;
        let width = (space - (columns - 1) as f32 * gap) / columns as f32;
        let now = ui.input(|input| input.time);
        let choices: Vec<_> = [Selection::UkraineFlag, Selection::UkraineTrident]
            .into_iter()
            .chain(ids.iter().copied().map(Selection::Firmware))
            .collect();

        egui::ScrollArea::vertical()
            .id_salt("built_in_scene_tiles")
            .max_height(321.0)
            .auto_shrink([false, true])
            .show_rows(ui, HEIGHT, choices.len().div_ceil(columns), |ui, rows| {
                let mut next_frame = f64::INFINITY;
                for row in rows {
                    ui.horizontal(|ui| {
                        for &choice in
                            &choices[row * columns..((row + 1) * columns).min(choices.len())]
                        {
                            ui.push_id(choice, |ui| {
                                let name = choice.name();
                                let is_selected = *selected == choice;
                                let response = ui.add_sized(
                                    [width, HEIGHT],
                                    Button::new("").selected(is_selected),
                                );
                                response.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::WidgetType::SelectableLabel,
                                        true,
                                        is_selected,
                                        &name,
                                    )
                                });
                                if response.clicked() {
                                    *selected = choice;
                                }
                                let card = response.rect;
                                let image_rect = Rect::from_center_size(
                                    pos2(card.center().x, card.top() + 37.0),
                                    vec2(62.0, 62.0),
                                );
                                ui.painter().rect_filled(image_rect, 3, crate::BACKGROUND);
                                if let Some(frames) = choice.frames() {
                                    let duration = choice.preview_frame_seconds(speed);
                                    if frames.len() > 1 {
                                        next_frame = next_frame.min(duration - now % duration);
                                    }
                                    let index = (now / duration) as usize % frames.len();
                                    let texture =
                                        self.textures.entry((choice, index)).or_insert_with(|| {
                                            ui.ctx().load_texture(
                                                format!("scene-{choice:?}-{index}"),
                                                egui::ColorImage::new(
                                                    [crate::protocol::EDGE; 2],
                                                    frames[index].clone(),
                                                ),
                                                TextureOptions::NEAREST,
                                            )
                                        });
                                    ui.painter().image(
                                        texture.id(),
                                        image_rect,
                                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                    if let Some(category) = choice.category() {
                                        response.on_hover_text(format!("{name} · {category}"));
                                    }
                                } else if let Selection::Firmware(id) = choice {
                                    ui.painter().text(
                                        image_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        id,
                                        FontId::monospace(17.0),
                                        crate::MUTED,
                                    );
                                    response
                                        .on_hover_text(format!("Scene {id} · preview on curtain"));
                                }
                                let mut label = egui::text::LayoutJob::simple_singleline(
                                    name.into_owned(),
                                    FontId::proportional(11.0),
                                    crate::TEXT,
                                );
                                label.wrap.max_width = card.width() - 8.0;
                                label.wrap.max_rows = 1;
                                let galley = ui.painter().layout_job(label);
                                let origin = pos2(
                                    card.center().x - galley.size().x / 2.0,
                                    card.bottom() - 17.0,
                                );
                                ui.painter().galley(origin, galley, crate::TEXT);
                            });
                        }
                    });
                }
                if next_frame.is_finite() {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_secs_f64(next_frame));
                }
            });
    }
}
