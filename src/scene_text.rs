use eframe::egui::Color32;

use crate::{
    protocol::{EDGE, PIXELS},
    scenes::ScenePreview,
};

const FIRST_ID: u8 = 58;
const LAST_ID: u8 = 63;
const GLYPH_WIDTH: usize = if cfg!(hypelights_vendor_font) { 8 } else { 4 };
const GLYPH_HEIGHT: usize = if cfg!(hypelights_vendor_font) { 16 } else { 7 };
#[cfg(hypelights_vendor_font)]
const FONT: &[u8; 95 * GLYPH_HEIGHT] = include_bytes!("../assets/scene-text-font.bin");

// Hello Fairy's 20 × 20 text theme lists these messages in slot order.
const TITLES: [&str; 6] = [
    "Hello Fairy",
    "Welcome",
    "Merry Christmas",
    "Happy New Year",
    "Good Luck",
    "Happy Birthday",
];
const GRADIENTS: [[[u8; 3]; 3]; 3] = [
    [[255, 255, 0], [255, 0, 0], [255, 0, 255]],
    [[255, 0, 255], [0, 0, 255], [0, 255, 255]],
    [[255, 255, 0], [0, 255, 0], [0, 255, 255]],
];

pub fn contains(id: u8) -> bool {
    (FIRST_ID..=LAST_ID).contains(&id)
}

pub fn previews() -> impl Iterator<Item = ScenePreview> {
    (FIRST_ID..=LAST_ID)
        .zip(TITLES)
        .enumerate()
        .map(|(index, (id, title))| ScenePreview {
            id,
            category: "Text".into(),
            name: title.into(),
            frames: scroll_frames(title, GRADIENTS[index % GRADIENTS.len()]),
        })
}

fn scroll_frames(text: &str, gradient: [[u8; 3]; 3]) -> Vec<Vec<Color32>> {
    let glyphs: Vec<_> = text.bytes().map(glyph).collect();
    let text_width = glyphs.len() * GLYPH_WIDTH;
    let cycle = text_width + EDGE;
    let colors: [Color32; EDGE] = std::array::from_fn(|column| gradient_color(gradient, column));
    (0..cycle)
        .map(|offset| {
            let mut frame = vec![Color32::BLACK; PIXELS];
            for (column, color) in colors.into_iter().enumerate() {
                let position = (offset + column) % cycle;
                let Some(glyph) = glyphs.get(position / GLYPH_WIDTH) else {
                    continue;
                };
                let mask = 1 << (GLYPH_WIDTH - 1 - position % GLYPH_WIDTH);
                for (row, bits) in glyph.iter().enumerate() {
                    if bits & mask != 0 {
                        frame[(row + (EDGE - GLYPH_HEIGHT) / 2) * EDGE + column] = color;
                    }
                }
            }
            frame
        })
        .collect()
}

#[cfg(hypelights_vendor_font)]
fn glyph(character: u8) -> [u8; GLYPH_HEIGHT] {
    let offset = (usize::from(character) - 32) * GLYPH_HEIGHT;
    FONT[offset..offset + GLYPH_HEIGHT]
        .try_into()
        .expect("fixed glyph height")
}

#[cfg(not(hypelights_vendor_font))]
fn glyph(character: u8) -> [u8; GLYPH_HEIGHT] {
    crate::starter_art::glyph(character.to_ascii_uppercase())
}

fn gradient_color(gradient: [[u8; 3]; 3], column: usize) -> Color32 {
    let width = EDGE - 1;
    let segment = (column * 2 / width).min(1);
    let step = column * 2 - segment * width;
    let channel = |i| {
        ((usize::from(gradient[segment][i]) * (width - step)
            + usize::from(gradient[segment + 1][i]) * step)
            / width) as u8
    };
    Color32::from_rgb(channel(0), channel(1), channel(2))
}
