pub const EDGE: usize = 20;
pub const PIXELS: usize = EDGE * EDGE;
pub type Pixels = [u8; PIXELS];
pub const OFF: u8 = 254;
pub const WHITE: u8 = 255;
#[cfg(windows)]
pub const FRAME_MTU: u16 = 245;

pub fn configured_address() -> String {
    std::env::var("HYPELIGHTS_DEVICE_ADDRESS").unwrap_or_else(|_| {
        option_env!("HYPELIGHTS_LOCAL_ADDRESS")
            .unwrap_or_default()
            .to_owned()
    })
}

#[derive(Clone, Copy)]
pub enum Lighting {
    Solid([u8; 3]),
    Scene(u8),
    Music {
        mode: u8,
        sensitivity: u8,
        rgb: [u8; 3],
        automatic: bool,
    },
}

#[derive(Clone)]
pub enum Command {
    Power(bool),
    Apply {
        lighting: Lighting,
        brightness: u8,
        scene_speed: Option<u8>,
    },
    TextScroll {
        text: &'static str,
        speed: u8,
        brightness: u8,
    },
    Picture {
        pixels: Pixels,
        movement: u8,
        speed: u8,
        brightness: u8,
    },
    Animation {
        frames: Box<[Pixels; 2]>,
        breathe: bool,
        speed: u8,
        brightness: u8,
    },
}

pub fn checksum(packet: &[u8]) -> u8 {
    packet.iter().copied().fold(0, u8::wrapping_add)
}

pub fn packet(command: u8, payload: &[u8]) -> Vec<u8> {
    let mut packet = vec![0xaa, command, payload.len() as u8];
    packet.extend_from_slice(payload);
    packet.push(checksum(&packet));
    packet
}

pub fn power(on: bool) -> Vec<u8> {
    packet(0x02, &[u8::from(on)])
}

pub fn lighting(lighting: Lighting, brightness: u8) -> Vec<u8> {
    let value = u16::from(brightness) * 10;
    let [value_high, value_low] = value.to_be_bytes();
    match lighting {
        Lighting::Solid(rgb) => {
            let (hue, saturation) = rgb_to_hs(rgb);
            let [hue_high, hue_low] = hue.to_be_bytes();
            let [sat_high, sat_low] = saturation.to_be_bytes();
            // Keep the chosen color's intensity proportional to the brightness control.
            let peak = u32::from(rgb[0].max(rgb[1]).max(rgb[2]));
            let scaled = ((u32::from(value) * peak + 127) / 255) as u16;
            let [value_high, value_low] = scaled.to_be_bytes();
            packet(
                0x03,
                &[
                    0x01, hue_high, hue_low, sat_high, sat_low, value_high, value_low,
                ],
            )
        }
        Lighting::Scene(number) => packet(0x03, &[0x02, number, value_high, value_low]),
        Lighting::Music {
            mode,
            sensitivity,
            rgb,
            automatic,
        } => {
            let (hue, saturation) = rgb_to_hs(rgb);
            let mut payload = vec![
                3,
                (mode << 4) | u8::from(automatic || mode == 1),
                sensitivity,
            ];
            payload.extend_from_slice(&value.to_be_bytes());
            payload.extend_from_slice(&hue.to_be_bytes());
            payload.extend_from_slice(&saturation.to_be_bytes());
            packet(0x03, &payload)
        }
    }
}

pub fn pixel_is_valid(color: u8) -> bool {
    color <= 180 || color == OFF || color == WHITE
}

pub fn picture_chunks(pixels: &Pixels) -> Option<Vec<Vec<u8>>> {
    picture_slot(pixels, 1)
}

fn picture_slot(pixels: &Pixels, slot: u8) -> Option<Vec<Vec<u8>>> {
    if !pixels.iter().copied().all(pixel_is_valid) {
        return None;
    }
    let mut packets = Vec::new();
    for first in (0..PIXELS).step_by(79) {
        let mut payload = vec![slot];
        for index in first..(first + 79).min(PIXELS) {
            let x = index / EDGE;
            let y = index % EDGE;
            payload.extend_from_slice(&(index as u16).to_be_bytes());
            payload.push(pixels[y * EDGE + x]);
        }
        packets.push(packet(0xda, &payload));
    }
    Some(packets)
}

pub fn scroll_text_packets(text: &str, speed: u8, brightness: u8) -> Option<Vec<Vec<u8>>> {
    if text.is_empty()
        || text.len() > 20
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
    {
        return None;
    }
    let mut packets = vec![
        packet(0xd3, &[1, brightness, speed, OFF, WHITE]),
        packet(0xd4, &[21, text.len() as u8, 0]),
    ];
    packets.extend(
        text.bytes()
            .enumerate()
            .map(|(index, letter)| packet(0xd4, &[index as u8 + 1, letter, WHITE])),
    );
    Some(packets)
}

pub fn picture_settings(movement: u8, brightness: u8, speed: u8, action: u8) -> Vec<u8> {
    packet(0xd0, &[movement, brightness, speed, action])
}

pub fn animation_packets(
    frames: &[Pixels; 2],
    breathe: bool,
    speed: u8,
    brightness: u8,
) -> Option<Vec<(Vec<u8>, u64)>> {
    let mut sequence = vec![(picture_settings(5, 100, 100, 7), 25)];
    for (index, frame) in frames.iter().enumerate() {
        let slot = index as u8 + 1;
        sequence.push((packet(0xd7, &[slot, 0]), 25));
        if slot == 1 {
            sequence.push((picture_settings(5, 100, 100, 0), 250));
        }
        sequence.extend(
            picture_slot(frame, slot)?
                .into_iter()
                .map(|packet| (packet, 300)),
        );
        sequence.push((packet(0xd7, &[slot, 2]), 25));
    }
    let mode = u8::from(breathe);
    sequence.push((picture_settings(5, 100, 100, 2), 25));
    sequence.push((packet(0xdc, &[0]), 200));
    if breathe || speed != 100 || brightness != 100 {
        sequence.push((picture_settings(mode, brightness, speed, 2), 25));
        sequence.push((packet(0xdc, &[mode]), 200));
    }
    Some(sequence)
}

#[cfg(any(windows, test))]
pub fn response_payload(reply: &[u8], command: u8) -> Option<&[u8]> {
    if reply.len() < 4
        || !reply.starts_with(&[0xaa, command])
        || reply.len() != usize::from(reply[2]) + 4
        || checksum(&reply[..reply.len() - 1]) != reply[reply.len() - 1]
    {
        return None;
    }
    Some(&reply[3..reply.len() - 1])
}

fn rgb_to_hs(rgb: [u8; 3]) -> (u16, u16) {
    let [red, green, blue] = rgb.map(f64::from);
    let maximum = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let range = maximum - minimum;
    if range == 0.0 {
        return (0, 0);
    }
    let hue = if maximum == red {
        (green - blue) / range
    } else if maximum == green {
        2.0 + (blue - red) / range
    } else {
        4.0 + (red - green) / range
    };
    (
        (hue * 60.0).round().rem_euclid(360.0) as u16,
        (1000.0 * range / maximum).round() as u16,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_power_and_scene_packets() {
        assert_eq!(power(true), [0xaa, 2, 1, 1, 0xae]);
        assert_eq!(power(false), [0xaa, 2, 1, 0, 0xad]);
        assert_eq!(
            lighting(Lighting::Scene(58), 50),
            [0xaa, 3, 4, 2, 58, 1, 0xf4, 0xe2]
        );
    }

    #[test]
    fn encodes_hsv_and_scales_brightness() {
        assert_eq!(
            lighting(Lighting::Solid([0, 0, 255]), 100),
            [0xaa, 3, 7, 1, 0, 240, 3, 232, 3, 232, 123]
        );
        let half = lighting(Lighting::Solid([0, 128, 0]), 100);
        assert_eq!(&half[4..10], &[0, 120, 3, 232, 1, 0xf6]);
        assert_eq!(&lighting(Lighting::Solid([0, 0, 0]), 100)[4..10], &[0; 6]);
        assert_eq!(rgb_to_hs([255, 0, 1]).0, 0);
    }

    #[test]
    fn encodes_full_picture_in_column_order() {
        let mut pixels = [OFF; PIXELS];
        pixels[1] = 120;
        pixels[EDGE] = WHITE;
        let packets = picture_chunks(&pixels).unwrap();
        assert_eq!(
            packets.iter().map(Vec::len).collect::<Vec<_>>(),
            [242, 242, 242, 242, 242, 20]
        );
        let first = response_payload(&packets[0], 0xda).unwrap();
        assert_eq!(&first[..7], &[1, 0, 0, OFF, 0, 1, WHITE]);
        assert_eq!(&first[61..64], &[0, 20, 120]);
        assert_eq!(
            picture_settings(3, 100, 20, 1),
            [0xaa, 0xd0, 4, 3, 100, 20, 1, 0xfa]
        );
    }

    #[test]
    fn encodes_curtain_microphone_mode() {
        let frame = lighting(
            Lighting::Music {
                mode: 1,
                sensitivity: 80,
                rgb: [255, 0, 0],
                automatic: false,
            },
            60,
        );
        assert_eq!(
            response_payload(&frame, 3),
            Some(&[3, 0x11, 80, 2, 88, 0, 0, 3, 232][..])
        );
    }
    #[test]
    fn animation_transfers_both_slots_before_playback() {
        let frames = [[OFF; PIXELS], [WHITE; PIXELS]];
        let sequence = animation_packets(&frames, false, 100, 100).unwrap();
        assert_eq!(sequence[0].0, [0xaa, 0xd0, 4, 5, 100, 100, 7, 0x52]);
        let slots: Vec<_> = sequence
            .iter()
            .filter_map(|(packet, _)| response_payload(packet, 0xd7))
            .collect();
        assert_eq!(slots, [&[1, 0], &[1, 2], &[2, 0], &[2, 2]]);
        let chunks: Vec<_> = sequence
            .iter()
            .filter_map(|(packet, _)| response_payload(packet, 0xda))
            .collect();
        assert_eq!(chunks.len(), 12);
        assert_eq!(&chunks[0][..4], &[1, 0, 0, OFF]);
        assert_eq!(&chunks[6][..4], &[2, 0, 0, WHITE]);
        assert_eq!(sequence.last().unwrap().0, [0xaa, 0xdc, 1, 0, 0x87]);
    }
    #[test]
    fn text_scroll_sends_the_whole_message_in_white() {
        let packets = scroll_text_packets("PINK FLAMINGO", 55, 75).unwrap();
        assert_eq!(
            response_payload(&packets[0], 0xd3).unwrap(),
            [1, 75, 55, OFF, WHITE]
        );
        assert_eq!(response_payload(&packets[1], 0xd4).unwrap(), [21, 13, 0]);
        let text: Vec<_> = packets[2..]
            .iter()
            .enumerate()
            .map(|(index, packet)| {
                let entry = response_payload(packet, 0xd4).unwrap();
                assert_eq!(entry[0], index as u8 + 1);
                assert_eq!(entry[2], WHITE);
                entry[1]
            })
            .collect();
        assert_eq!(text, b"PINK FLAMINGO");
    }
}
