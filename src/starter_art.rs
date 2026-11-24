use std::{
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::protocol::{EDGE, OFF, PIXELS, Pixels, WHITE};

pub const FLAMINGO: &str = "Pink Flamingo";
pub const MARQUEE: &str = "PINK FLAMINGO · Scroll";
pub const RANDOM: &str = "Random RGB";
pub const MESSAGE: &str = "PINK FLAMINGO";

pub fn flamingo() -> Vec<Pixels> {
    let body = [
        "....................",
        "..............pp....",
        ".............pPPp...",
        "............pP.WPW..",
        "............pPPWwss.",
        "............PPp.....",
        "...........pPp......",
        "...........Pp.......",
        "...........PP.......",
        "..s.......pPPp......",
        "..sp.....pPPPp......",
        "...sPppppPPPP.......",
        "....PPppPPpP........",
        ".....PmmmmP.........",
        "......smmP..........",
    ];
    [false, true]
        .map(|step| {
            let mut frame = [OFF; PIXELS];
            for (row, line) in body.iter().enumerate() {
                for (column, symbol) in line.bytes().enumerate() {
                    frame[row * EDGE + column] = match symbol {
                        b'p' => 173,
                        b'P' => 165,
                        b'm' => 154,
                        b's' => 145,
                        b'w' => 14,
                        b'W' => WHITE,
                        _ => OFF,
                    };
                }
            }
            if step {
                frame[3 * EDGE + 14] = 173;
                frame[3 * EDGE + 15] = 154;
                frame[11 * EDGE + 5] = 154;
            }
            let standing = if step { 10 } else { 8 };
            for row in 15..=18 {
                frame[row * EDGE + standing] = 171;
            }
            frame[18 * EDGE + standing + 1] = 171;
            let lifted = if step {
                [(8, 15), (7, 16), (6, 16)]
            } else {
                [(10, 15), (11, 16), (10, 16)]
            };
            for (x, y) in lifted {
                frame[y * EDGE + x] = 154;
            }
            for (x, y) in if step {
                [(4, 19), (5, 19), (12, 19), (13, 19), (16, 18)]
            } else {
                [(3, 19), (4, 19), (11, 19), (12, 19), (15, 18)]
            } {
                frame[y * EDGE + x] = 91;
            }
            frame
        })
        .to_vec()
}

pub fn marquee() -> &'static [Pixels] {
    static FRAMES: OnceLock<Vec<Pixels>> = OnceLock::new();
    FRAMES.get_or_init(|| {
        let mut columns = Vec::new();
        for letter in MESSAGE.bytes() {
            if letter == b' ' {
                columns.extend([0; 3]);
                continue;
            }
            let rows = glyph(letter);
            for x in (0..4).rev() {
                let column = rows
                    .iter()
                    .enumerate()
                    .fold(0, |mask, (row, bits)| mask | (((bits >> x) & 1) << row));
                columns.push(column);
            }
            columns.push(0);
        }
        columns.extend([0; 8]);
        (0..columns.len())
            .map(|offset| {
                let mut frame = [OFF; PIXELS];
                for x in 0..EDGE {
                    let column = columns[(offset + x + columns.len() - EDGE) % columns.len()];
                    for y in 0..7 {
                        if column & (1 << y) != 0 {
                            frame[(y + 6) * EDGE + x] = WHITE;
                        }
                    }
                }
                frame
            })
            .collect()
    })
}

pub(crate) fn glyph(letter: u8) -> [u8; 7] {
    match letter {
        b'P' => [14, 9, 9, 14, 8, 8, 8],
        b'I' => [14, 4, 4, 4, 4, 4, 14],
        b'N' => [9, 13, 13, 11, 11, 9, 9],
        b'K' => [9, 10, 12, 8, 12, 10, 9],
        b'F' => [15, 8, 8, 14, 8, 8, 8],
        b'L' => [8, 8, 8, 8, 8, 8, 15],
        b'A' => [6, 9, 9, 15, 9, 9, 9],
        b'M' => [9, 15, 15, 9, 9, 9, 9],
        b'G' => [6, 9, 8, 11, 9, 9, 7],
        b'O' => [6, 9, 9, 9, 9, 9, 6],
        b'B' => [14, 9, 9, 14, 9, 9, 14],
        b'C' => [7, 8, 8, 8, 8, 8, 7],
        b'D' => [14, 9, 9, 9, 9, 9, 14],
        b'E' => [15, 8, 8, 14, 8, 8, 15],
        b'H' => [9, 9, 9, 15, 9, 9, 9],
        b'R' => [14, 9, 9, 14, 10, 9, 9],
        b'S' => [7, 8, 8, 6, 1, 1, 14],
        b'T' => [15, 4, 4, 4, 4, 4, 4],
        b'U' => [9, 9, 9, 9, 9, 9, 6],
        b'W' => [9, 9, 9, 9, 15, 15, 9],
        b'Y' => [9, 9, 9, 6, 4, 4, 4],
        b' ' => [0; 7],
        _ => unreachable!("preview text uses the bundled glyphs"),
    }
}

pub fn random_rgb() -> Vec<Pixels> {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |elapsed| elapsed.as_nanos() as u64);
    random_frames(seed.max(1))
}

fn random_frames(mut state: u64) -> Vec<Pixels> {
    let mut next_color = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 180) as u8
    };
    let first = std::array::from_fn(|_| next_color());
    let second = std::array::from_fn(|index| {
        let color = next_color();
        if color == first[index] {
            (color + 1) % 180
        } else {
            color
        }
    });
    vec![first, second]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_animations_are_valid_and_move() {
        let flamingo = flamingo();
        assert_ne!(flamingo[0], flamingo[1]);
        assert!(flamingo[0].iter().filter(|&&pixel| pixel == 165).count() > 20);
        let marquee = marquee();
        assert!(marquee.len() <= crate::library::MAX_FRAMES);
        assert!(
            marquee
                .iter()
                .flatten()
                .all(|&pixel| pixel == OFF || pixel == WHITE)
        );
        assert!(marquee.iter().any(|frame| frame.contains(&WHITE)));
        for (index, frame) in marquee.iter().enumerate() {
            let next = &marquee[(index + 1) % marquee.len()];
            for row in 0..EDGE {
                assert_eq!(
                    &frame[row * EDGE + 1..(row + 1) * EDGE],
                    &next[row * EDGE..(row + 1) * EDGE - 1]
                );
            }
        }
        let colors = random_frames(42);
        assert!(
            colors[0]
                .iter()
                .zip(&colors[1])
                .all(|(&a, &b)| a < 180 && b < 180 && a != b)
        );
        assert!(
            colors[0]
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                > 140
        );
    }
}
