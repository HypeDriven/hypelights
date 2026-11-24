use std::{fs, io, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::{PIXELS, pixel_is_valid};

pub const PRESET_COUNT: u8 = 58;
pub const MAX_FRAMES: usize = 96;
pub const DEFAULT_FRAME_MS: u16 = 500;

fn default_frame_ms() -> u16 {
    DEFAULT_FRAME_MS
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Presets {
    pub model: String,
    pub firmware: String,
    pub ids: Vec<u8>,
    pub last_scene: Option<u8>,
    pub scene_speed: Option<u8>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Artwork {
    pub name: String,
    pub pixels: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_frames: Vec<Vec<u8>>,
    #[serde(default)]
    pub breathe: bool,
    #[serde(default = "default_frame_ms")]
    pub frame_ms: u16,
    pub movement: u8,
    pub speed: u8,
    pub brightness: u8,
}

impl Artwork {
    pub fn is_valid(&self) -> bool {
        let valid_frame =
            |frame: &Vec<u8>| frame.len() == PIXELS && frame.iter().copied().all(pixel_is_valid);
        !self.name.trim().is_empty()
            && valid_frame(&self.pixels)
            && self.extra_frames.len() < MAX_FRAMES
            && self.extra_frames.iter().all(valid_frame)
            && (100..=5000).contains(&self.frame_ms)
            && self.movement <= 10
            && self.speed <= 100
            && (1..=100).contains(&self.brightness)
    }
}

pub struct Library {
    directory: Option<PathBuf>,
}

impl Library {
    pub fn new(demo: bool) -> Self {
        let directory = if cfg!(windows) {
            std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        } else {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        };
        let directory = directory.map(|path| {
            path.join(if demo {
                "Hypelights Demo"
            } else {
                "Hypelights"
            })
        });
        Self { directory }
    }

    pub fn presets(&self) -> io::Result<Presets> {
        self.read("presets.json")
    }

    pub fn artworks(&self) -> io::Result<Vec<Artwork>> {
        let saved: Vec<Artwork> = self.read("artwork.json")?;
        if saved.iter().all(Artwork::is_valid) {
            Ok(saved)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid artwork data",
            ))
        }
    }

    pub fn save_presets(&self, presets: &Presets) -> io::Result<()> {
        self.write("presets.json", presets)
    }

    pub fn save_artworks(&self, artwork: &[Artwork]) -> io::Result<()> {
        self.write("artwork.json", &artwork)
    }

    fn path(&self, name: &str) -> io::Result<PathBuf> {
        self.directory
            .as_ref()
            .map(|dir| dir.join(name))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "Could not locate the app data folder",
                )
            })
    }

    fn read<T: serde::de::DeserializeOwned>(&self, name: &str) -> io::Result<T> {
        let bytes = fs::read(self.path(name)?)?;
        serde_json::from_slice(&bytes).map_err(io::Error::other)
    }

    fn write(&self, name: &str, value: &impl Serialize) -> io::Result<()> {
        let path = self.path(name)?;
        let data = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
        fs::create_dir_all(path.parent().expect("library file has a parent directory"))?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&temporary, data)?;
        fs::rename(temporary, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::OFF;

    #[test]
    fn existing_artwork_loads_as_one_frame() {
        let old = serde_json::json!({
            "name": "Old design", "pixels": vec![OFF; PIXELS],
            "movement": 0, "speed": 50, "brightness": 75
        });
        let mut artwork: Artwork = serde_json::from_value(old).unwrap();
        assert!(artwork.is_valid());
        assert!(artwork.extra_frames.is_empty());
        assert_eq!(artwork.frame_ms, DEFAULT_FRAME_MS);
        artwork.extra_frames.push(vec![OFF]);
        assert!(!artwork.is_valid());
    }
}
