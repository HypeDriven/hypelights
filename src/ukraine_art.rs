use crate::protocol::{EDGE, OFF, PIXELS, Pixels};

pub const BLUE: u8 = 106;
pub const YELLOW: u8 = 25;

const FLAG_WAVE: [i8; 18] = [0, 0, -1, -1, -1, 0, 0, 1, 1, 2, 2, 1, 1, 0, -1, -2, -2, -1];
pub const AMBER: u8 = 19;

pub fn flag() -> [Pixels; 2] {
    [1, -1].map(|phase| {
        let mut pixels = [OFF; PIXELS];
        for (index, offset) in FLAG_WAVE.into_iter().enumerate() {
            let top = (4 + phase * offset) as usize;
            for row in 0..12 {
                pixels[(top + row) * EDGE + index + 1] = if row < 6 { BLUE } else { YELLOW };
            }
        }
        pixels
    })
}

// The 20 × 20 adaptation uses the Tryzub outline credited in README.md.
pub fn trident() -> [Pixels; 2] {
    let emblem = [
        ".....YY.....",
        "Y....YY....Y",
        "YY...YY...YY",
        "YYY..YY..YYY",
        "Y.Y..YY..Y.Y",
        "Y.Y..YY..Y.Y",
        "Y.YY.YY.YY.Y",
        "Y.YY.YY.YY.Y",
        "Y.YY.YY.YY.Y",
        "YYYY.YY.YYYY",
        "YYY.YYYY.YYY",
        "Y.YYYYYYYY.Y",
        "Y..YYYYYY..Y",
        "YYYYYYYYYYYY",
        "YYYYYYYYYYYY",
        "....YYYY....",
        "....YYYY....",
        ".....YY.....",
    ];
    let shield_insets = [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 5, 7, 9];
    [7, 13].map(|glint| {
        let mut pixels = [OFF; PIXELS];
        for (row, inset) in shield_insets.into_iter().enumerate() {
            pixels[row * EDGE + inset..(row + 1) * EDGE - inset].fill(BLUE);
        }
        for (row, stripe) in emblem.iter().enumerate() {
            for (column, color) in stripe.bytes().enumerate() {
                if color == b'Y' {
                    let diagonal = column + row / 2;
                    pixels[(row + 1) * EDGE + column + 4] = match diagonal.abs_diff(glint) {
                        0 => crate::protocol::WHITE,
                        1 => AMBER,
                        _ => YELLOW,
                    };
                }
            }
        }
        pixels
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_waves_with_equal_blue_and_yellow_bands() {
        let frames = flag();
        assert_ne!(frames[0], frames[1]);
        for pixels in frames {
            for column in 1..19 {
                let band: Vec<_> = (0..EDGE)
                    .map(|row| pixels[row * EDGE + column])
                    .filter(|&color| color != OFF)
                    .collect();
                assert_eq!(
                    band,
                    [BLUE; 6].into_iter().chain([YELLOW; 6]).collect::<Vec<_>>()
                );
            }
            assert!(crate::protocol::picture_chunks(&pixels).is_some());
        }
    }

    #[test]
    fn trident_glints_preserve_the_outline() {
        let frames = trident();
        assert_ne!(frames[0], frames[1]);
        let is_emblem = |pixel: u8| matches!(pixel, YELLOW | AMBER | crate::protocol::WHITE);
        assert_eq!(frames[0].map(is_emblem), frames[1].map(is_emblem));
        for pixels in frames {
            assert!(pixels.contains(&crate::protocol::WHITE));
            assert!(
                pixels
                    .map(is_emblem)
                    .chunks_exact(EDGE)
                    .all(|row| row.iter().eq(row.iter().rev()))
            );
            assert!(crate::protocol::picture_chunks(&pixels).is_some());
        }
    }
}
