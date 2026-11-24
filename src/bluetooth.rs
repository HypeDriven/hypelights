use std::time::Duration;

use btleplug::{
    api::{BDAddr, Central, CentralEvent, CentralState, Manager as _, Peripheral as _, ScanFilter},
    platform::Manager,
};
use futures_util::StreamExt;
use std::sync::mpsc;
use tokio::{runtime::Runtime, sync::mpsc as async_mpsc, time};
use windows::{
    Devices::Bluetooth::{
        BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice,
        GenericAttributeProfile::{
            GattCharacteristic, GattCharacteristicProperties,
            GattClientCharacteristicConfigurationDescriptorValue as Subscription,
            GattCommunicationStatus, GattDeviceService, GattValueChangedEventArgs, GattWriteOption,
        },
    },
    Foundation::TypedEventHandler,
    Storage::Streams::{DataReader, DataWriter},
    core::GUID,
};

use crate::{
    Event, Request, Snapshot,
    library::{PRESET_COUNT, Presets},
    protocol::{self, Command, FRAME_MTU, PIXELS},
};

const SERVICE: GUID = GUID::from_u128(0x49535343_fe7d_4ae5_8fa9_9fafd205e455);
const WRITE: GUID = GUID::from_u128(0x49535343_8841_43f4_a8d4_ecbe34729bb3);
const NOTIFY: GUID = GUID::from_u128(0x49535343_1e4d_4bd9_ba61_23c647249616);

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn start(
    context: eframe::egui::Context,
    events: mpsc::Sender<Event>,
) -> async_mpsc::UnboundedSender<Request> {
    let (requests, receiver) = async_mpsc::unbounded_channel();
    std::thread::spawn(move || {
        let event = |message| {
            let _ = events.send(message);
            context.request_repaint();
        };
        match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => run(runtime, receiver, event),
            Err(error) => event(Event::Disconnected(format!(
                "Bluetooth startup failed: {error}"
            ))),
        }
    });
    requests
}

fn run(
    runtime: Runtime,
    mut requests: async_mpsc::UnboundedReceiver<Request>,
    event: impl Fn(Event),
) {
    runtime.block_on(async {
        let mut link: Option<Link> = None;
        loop {
            let request = match time::timeout(Duration::from_secs(2), requests.recv()).await {
                Ok(Some(request)) => request,
                Ok(None) => break,
                Err(_) => {
                    if let Some(active) = &link
                        && matches!(
                            active.device.ConnectionStatus(),
                            Ok(BluetoothConnectionStatus::Disconnected)
                        )
                    {
                        link = None;
                        event(Event::Disconnected("Bluetooth connection lost.".into()));
                    }
                    continue;
                }
            };
            match request {
                Request::Connect(address) => match Link::connect(&address).await {
                    Ok(mut active) => {
                        let snapshot = active.inspect().await;
                        link = Some(active);
                        event(Event::Connected(snapshot));
                    }
                    Err(error) => event(Event::Disconnected(error.to_string())),
                },
                Request::Disconnect => {
                    drop(link.take());
                    event(Event::Disconnected("Disconnected.".into()));
                }
                Request::Send(command) => {
                    let Some(active) = &mut link else {
                        event(Event::Disconnected("Connect your curtain first.".into()));
                        continue;
                    };
                    match active.send(&command).await {
                        Ok(()) => event(Event::Sent(command)),
                        Err(error) => {
                            let message = format!("{error} Reconnect to try again.");
                            drop(link.take());
                            event(Event::Disconnected(message));
                        }
                    }
                }
            }
        }
    });
}

struct Link {
    device: BluetoothLEDevice,
    service: GattDeviceService,
    write: GattCharacteristic,
    write_type: GattWriteOption,
    notify: GattCharacteristic,
    notify_token: i64,
    notifications: async_mpsc::UnboundedReceiver<Vec<u8>>,
    picture_supported: bool,
}

impl Link {
    async fn connect(address: &str) -> Result<Self> {
        let address = address
            .parse::<BDAddr>()
            .map_err(|_| "Enter a Bluetooth address in the form XX:XX:XX:XX:XX:XX.")?;
        Self::discover(address).await?;
        time::timeout(Duration::from_secs(20), Self::setup(address.into()))
            .await
            .map_err(|_| "The curtain timed out while connecting.")?
    }

    async fn discover(address: BDAddr) -> Result<()> {
        let manager = Manager::new().await?;
        let adapter = manager
            .adapters()
            .await?
            .into_iter()
            .next()
            .ok_or("Windows could not find a Bluetooth LE adapter.")?;
        if adapter.adapter_state().await? == CentralState::PoweredOff {
            return Err("Turn on Bluetooth in Windows Settings and try again.".into());
        }
        let mut events = adapter.events().await?;
        adapter.start_scan(ScanFilter::default()).await?;
        let discovery = time::timeout(Duration::from_secs(10), async {
            while let Some(event) = events.next().await {
                if let CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id) = event
                    && let Ok(device) = adapter.peripheral(&id).await
                    && device.address() == address
                {
                    return true;
                }
            }
            false
        })
        .await;
        adapter.stop_scan().await?;
        if discovery.unwrap_or(false) {
            Ok(())
        } else {
            Err("Curtain not found. Check that it is powered, nearby, and disconnected from Hello Fairy on your phone.".into())
        }
    }

    async fn setup(address: u64) -> Result<Self> {
        let device = BluetoothLEDevice::FromBluetoothAddressAsync(address)?.await?;
        let services = device
            .GetGattServicesForUuidWithCacheModeAsync(SERVICE, BluetoothCacheMode::Uncached)?
            .await?;
        check_status(services.Status()?, "Finding the Hello Fairy service")?;
        let service = services
            .Services()?
            .into_iter()
            .next()
            .ok_or("The device did not expose the Hello Fairy service.")?;
        let result = service
            .GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Uncached)?
            .await?;
        check_status(result.Status()?, "Finding the curtain controls")?;
        let mut write = None;
        let mut notify = None;
        for characteristic in result.Characteristics()? {
            match characteristic.Uuid()? {
                WRITE => write = Some(characteristic),
                NOTIFY => notify = Some(characteristic),
                _ => {}
            }
        }
        let write = write.ok_or("The curtain did not expose its control characteristic.")?;
        let notify = notify.ok_or("The curtain did not expose its notification characteristic.")?;
        let properties = write.CharacteristicProperties()?;
        let write_type = if properties.contains(GattCharacteristicProperties::WriteWithoutResponse)
        {
            GattWriteOption::WriteWithoutResponse
        } else if properties.contains(GattCharacteristicProperties::Write) {
            GattWriteOption::WriteWithResponse
        } else {
            return Err("The Hello Fairy control characteristic is read-only.".into());
        };
        let (sender, notifications) = async_mpsc::unbounded_channel();
        let handler = TypedEventHandler::<GattCharacteristic, GattValueChangedEventArgs>::new(
            move |_, args| {
                if let Some(args) = args.as_ref() {
                    let reader = DataReader::FromBuffer(&args.CharacteristicValue()?)?;
                    let mut packet = vec![0; reader.UnconsumedBufferLength()? as usize];
                    reader.ReadBytes(&mut packet)?;
                    let _ = sender.send(packet);
                }
                Ok(())
            },
        );
        let notify_token = notify.ValueChanged(&handler)?;
        let link = Self {
            device,
            service,
            write,
            write_type,
            notify,
            notify_token,
            notifications,
            picture_supported: false,
        };
        // This curtain advertises both modes but returns ATT error 0x80 for indications.
        let subscription = link
            .notify
            .WriteClientCharacteristicConfigurationDescriptorWithResultAsync(Subscription::Notify)?
            .await?;
        check_status(subscription.Status()?, "Enabling curtain notifications")?;
        Ok(link)
    }

    async fn inspect(&mut self) -> Snapshot {
        let information = self.query(0x00, 27).await;
        let status = self.query(0x01, 5).await;
        let scene_speed = self
            .query(0x0c, 1)
            .await
            .and_then(|data| (data.len() == 1 && data[0] <= 100).then_some(data[0]));
        let power = status.as_deref().and_then(|data| match data[3] {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        });
        let active_scene = status.as_deref().and_then(|data| {
            data.get(5)
                .copied()
                .filter(|id| data[4] == 2 && (1..200).contains(id))
        });
        let mut snapshot = Snapshot {
            power,
            ..Snapshot::default()
        };
        if let Some(info) = information {
            let model = String::from_utf8_lossy(&info[5..20])
                .trim_end_matches('\0')
                .to_owned();
            let firmware = String::from_utf8_lossy(&info[..5]).to_string();
            snapshot.music = model == "BESL18P";
            self.picture_supported = model == "BESL18P"
                && info[20..23] == [1, 144, 20]
                && info[24] == 20
                && info[26] == 3;
            snapshot.picture = self.picture_supported && self.has_picture_capacity();
            snapshot.presets = Some(Presets {
                model,
                firmware,
                ids: (1..=PRESET_COUNT.max(active_scene.unwrap_or_default())).collect(),
                last_scene: active_scene,
                scene_speed,
            });
        }
        snapshot
    }

    async fn query(&mut self, command: u8, minimum_length: usize) -> Option<Vec<u8>> {
        let request = protocol::packet(command, &[]);
        let response = time::timeout(
            Duration::from_millis(1400),
            self.exchange(&request, |reply| {
                protocol::response_payload(reply, command)
                    .is_some_and(|payload| payload.len() >= minimum_length)
            }),
        )
        .await
        .ok()?
        .ok()?;
        protocol::response_payload(&response, command).map(<[u8]>::to_vec)
    }

    fn has_picture_capacity(&self) -> bool {
        self.write_type == GattWriteOption::WriteWithoutResponse
            && self
                .service
                .Session()
                .and_then(|session| session.MaxPduSize())
                .is_ok_and(|mtu| mtu >= FRAME_MTU)
    }

    async fn send_picture(
        &mut self,
        pixels: &[u8; PIXELS],
        movement: u8,
        speed: u8,
        brightness: u8,
    ) -> Result<()> {
        if !self.picture_supported || !self.has_picture_capacity() {
            return Err(
                "The current Bluetooth connection cannot transfer 20 × 20 pictures.".into(),
            );
        }
        let packets =
            protocol::picture_chunks(pixels).ok_or("The picture contains unsupported colors.")?;
        self.write_and_confirm(protocol::power(true)).await?;
        time::sleep(Duration::from_millis(100)).await;
        self.write_packet(&protocol::picture_settings(0, 100, 100, 3))
            .await?;
        time::sleep(Duration::from_millis(17)).await;
        self.write_packet(&protocol::picture_settings(0, 100, 100, 0))
            .await?;
        time::sleep(Duration::from_millis(250)).await;
        for (index, packet) in packets.iter().enumerate() {
            if index != 0 {
                time::sleep(Duration::from_millis(300)).await;
            }
            self.write_packet(packet).await?;
        }
        time::sleep(Duration::from_millis(1200)).await;
        self.write_packet(&protocol::picture_settings(0, 100, 100, 4))
            .await?;
        time::sleep(Duration::from_millis(20)).await;
        self.write_packet(&protocol::picture_settings(movement, brightness, speed, 1))
            .await?;
        time::sleep(Duration::from_millis(200)).await;
        Ok(())
    }

    async fn send_animation(
        &mut self,
        frames: &[protocol::Pixels; 2],
        breathe: bool,
        speed: u8,
        brightness: u8,
    ) -> Result<()> {
        if !self.picture_supported || !self.has_picture_capacity() {
            return Err(
                "The current Bluetooth connection cannot transfer animation frames.".into(),
            );
        }
        let sequence = protocol::animation_packets(frames, breathe, speed, brightness)
            .ok_or("An animation frame contains unsupported colors.")?;
        self.write_and_confirm(protocol::power(true)).await?;
        time::sleep(Duration::from_millis(100)).await;
        let playback_ack = protocol::packet(0xdc, &[]);
        for (packet, delay_ms) in sequence {
            if packet[1] == 0xdc {
                // The curtain can append a sequence byte or combine this ACK with another.
                self.exchange(&packet, |reply| {
                    reply
                        .windows(playback_ack.len())
                        .any(|frame| frame == playback_ack)
                })
                .await?;
            } else {
                self.write_packet(&packet).await?;
            }
            time::sleep(Duration::from_millis(delay_ms)).await;
        }
        Ok(())
    }

    async fn send_scroll_text(&mut self, text: &str, speed: u8, brightness: u8) -> Result<()> {
        let packets = protocol::scroll_text_packets(text, speed, brightness)
            .ok_or("Scrolling text requires 1–20 ASCII characters.")?;
        self.write_and_confirm(protocol::power(true)).await?;
        time::sleep(Duration::from_millis(100)).await;
        for packet in packets {
            let ack = protocol::packet(packet[1], &[]);
            self.exchange(&packet, |reply| {
                reply.windows(ack.len()).any(|frame| frame == ack)
            })
            .await?;
            time::sleep(Duration::from_millis(50)).await;
        }
        if self
            .query(1, 8)
            .await
            .is_some_and(|state| state[3..6] == [1, 2, 0xc8])
        {
            Ok(())
        } else {
            Err("The curtain did not confirm scrolling text mode.".into())
        }
    }

    async fn set_scene_speed(&mut self, speed: u8) -> Result<()> {
        self.write_packet(&protocol::packet(0x0c, &[speed])).await?;
        time::sleep(Duration::from_millis(150)).await;
        if self.query(0x0c, 1).await.as_deref() == Some(&[speed]) {
            Ok(())
        } else {
            Err("The curtain did not confirm the scene speed.".into())
        }
    }

    async fn send(&mut self, command: &Command) -> Result<()> {
        match command {
            Command::Power(on) => self.write_and_confirm(protocol::power(*on)).await,
            Command::TextScroll {
                text,
                speed,
                brightness,
            } => self.send_scroll_text(text, *speed, *brightness).await,
            Command::Animation {
                frames,
                breathe,
                speed,
                brightness,
            } => {
                self.send_animation(frames, *breathe, *speed, *brightness)
                    .await
            }
            Command::Picture {
                pixels,
                movement,
                speed,
                brightness,
            } => {
                self.send_picture(pixels, *movement, *speed, *brightness)
                    .await
            }
            Command::Apply {
                lighting,
                brightness,
                scene_speed,
            } => {
                self.write_and_confirm(protocol::power(true)).await?;
                time::sleep(Duration::from_millis(100)).await;
                self.write_and_confirm(protocol::lighting(*lighting, *brightness))
                    .await?;
                if let (protocol::Lighting::Scene(_), Some(speed)) = (lighting, scene_speed) {
                    self.set_scene_speed(*speed).await?;
                }
                Ok(())
            }
        }
    }

    async fn write_and_confirm(&mut self, packet: Vec<u8>) -> Result<()> {
        let mut ack = [0xaa, packet[1], 0, 0];
        ack[3] = protocol::checksum(&ack[..3]);
        self.exchange(&packet, |reply| reply == ack).await?;
        Ok(())
    }

    async fn exchange(
        &mut self,
        packet: &[u8],
        matches: impl Fn(&[u8]) -> bool,
    ) -> Result<Vec<u8>> {
        while self.notifications.try_recv().is_ok() {}
        time::timeout(Duration::from_secs(5), async {
            self.write_packet(packet).await?;
            while let Some(reply) = self.notifications.recv().await {
                if matches(&reply) {
                    return Ok(reply);
                }
            }
            Err("The Bluetooth notification channel closed.".into())
        })
        .await
        .map_err(|_| "The curtain did not reply.")?
    }
    async fn write_packet(&self, packet: &[u8]) -> Result<()> {
        time::timeout(Duration::from_secs(5), async {
            let writer = DataWriter::new()?;
            writer.WriteBytes(packet)?;
            let data = writer.DetachBuffer()?;
            let status = self
                .write
                .WriteValueWithOptionAsync(&data, self.write_type)?
                .await?;
            check_status(status, "Writing to the curtain")
        })
        .await
        .map_err(|_| "Bluetooth write timed out.")?
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        let _ = self.notify.RemoveValueChanged(self.notify_token);
        let _ = self.service.Close();
        let _ = self.device.Close();
    }
}

fn check_status(status: GattCommunicationStatus, operation: &str) -> Result<()> {
    match status {
        GattCommunicationStatus::Success => Ok(()),
        GattCommunicationStatus::Unreachable => {
            Err(format!("{operation} failed because the curtain is unreachable.").into())
        }
        GattCommunicationStatus::AccessDenied => {
            Err(format!("{operation} was denied by Windows.").into())
        }
        _ => Err(format!("{operation} failed with Bluetooth status {}.", status.0).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the BESL18P to be powered and nearby"]
    fn connects_to_curtain() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut link = Link::connect(&protocol::configured_address())
                .await
                .expect("connect to the BESL18P");
            let snapshot = link.inspect().await;
            let presets = snapshot.presets.expect("read device information");
            println!(
                "{} firmware {}, custom picture {}, preset speed {:?}",
                presets.model, presets.firmware, snapshot.picture, presets.scene_speed
            );
            assert_eq!(presets.model, "BESL18P");
            assert!(snapshot.power.is_some(), "read the curtain status");
            assert!(snapshot.picture, "20 × 20 picture upload capability");
        });
    }

    #[test]
    #[ignore = "sends a temporary test picture to the physical curtain"]
    fn uploads_test_picture() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut link = Link::connect(&protocol::configured_address())
                .await
                .expect("connect");
            let snapshot = link.inspect().await;
            assert!(snapshot.picture);
            let before = link.query(1, 11).await.expect("read current lighting");
            assert_eq!(before[3..5], [1, 1], "only run while solid lighting is on");
            let image: [u8; PIXELS] =
                std::array::from_fn(|i| if i % 7 == 0 { protocol::WHITE } else { 140 });
            let upload = link.send_picture(&image, 0, 50, 75).await;
            let during = link.query(1, 5).await;
            let restore = link
                .write_and_confirm(protocol::packet(3, &before[4..]))
                .await;
            restore.expect("restore previous solid lighting");
            upload.expect("upload custom picture");
            println!("Status after picture transfer {during:02X?}");
        });
    }

    #[test]
    #[ignore = "temporarily selects a built-in scene and microphone mode"]
    fn checks_preset_and_music() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut link = Link::connect(&protocol::configured_address())
                .await
                .expect("connect");
            let before = link.query(1, 11).await.expect("read current lighting");
            assert_eq!(before[3..5], [1, 1], "only run while solid lighting is on");
            let scene = link
                .write_and_confirm(protocol::lighting(protocol::Lighting::Scene(97), 75))
                .await;
            let scene_reply = link.query(1, 5).await;
            let music = link
                .write_and_confirm(protocol::lighting(
                    protocol::Lighting::Music {
                        mode: 1,
                        sensitivity: 65,
                        automatic: true,
                        rgb: [255; 3],
                    },
                    75,
                ))
                .await;
            let music_reply = link.query(1, 5).await;
            link.write_and_confirm(protocol::packet(3, &before[4..]))
                .await
                .expect("restore previous solid lighting");
            println!("Preset: {scene_reply:02X?}; music: {music_reply:02X?}");
            scene.expect("preset acknowledgment");
            music.expect("music acknowledgment");
            assert_eq!(scene_reply.unwrap()[4..6], [2, 97]);
            assert_eq!(music_reply.unwrap()[4..7], [3, 0x11, 65]);
        });
    }
    #[test]
    #[ignore = "temporarily tests animations on the physical curtain"]
    fn checks_animations() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut link = Link::connect(&protocol::configured_address())
                .await
                .expect("connect");
            assert!(link.inspect().await.picture);
            let (was_on, settings) = saved_test_lighting(&mut link).await;
            let flamingo: [protocol::Pixels; 2] =
                crate::starter_art::flamingo().try_into().unwrap();
            let random: [protocol::Pixels; 2] =
                crate::starter_art::random_rgb().try_into().unwrap();
            let flamingo_result = link.send_animation(&flamingo, false, 45, 75).await;
            let random_result = link.send_animation(&random, false, 40, 75).await;
            restore_test_lighting(&mut link, was_on, &settings).await;
            flamingo_result.expect("upload Pink Flamingo");
            random_result.expect("upload Random RGB");
            println!("The curtain acknowledged both starter animations.");
        });
    }
    #[test]
    #[ignore = "temporarily sends scrolling text and a preset to the curtain"]
    fn plays_marquee_and_sets_preset_speed() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut link = Link::connect(&protocol::configured_address())
                .await
                .expect("connect");
            let (was_on, settings) = saved_test_lighting(&mut link).await;
            let speed = link
                .query(0x0c, 1)
                .await
                .and_then(|reply| reply.first().copied());
            let text = link
                .send(&Command::TextScroll {
                    text: crate::starter_art::MESSAGE,
                    speed: 55,
                    brightness: 75,
                })
                .await;
            let changed = link
                .send(&Command::Apply {
                    lighting: protocol::Lighting::Scene(1),
                    brightness: 75,
                    scene_speed: Some(33),
                })
                .await;
            if let Some(speed) = speed {
                link.set_scene_speed(speed).await.expect("restore speed");
            }
            restore_test_lighting(&mut link, was_on, &settings).await;
            text.expect("curtain confirmed native scrolling text");
            changed.expect("curtain confirmed scene speed");
        });
    }
    #[test]
    #[ignore = "temporarily displays the Ukrainian flag and trident on the curtain"]
    fn checks_ukrainian_scenes() {
        use crate::scenes::Selection;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut link = Link::connect(&protocol::configured_address())
                .await
                .expect("connect");
            assert!(link.inspect().await.picture);
            let (was_on, settings) = saved_test_lighting(&mut link).await;
            let flag = link
                .send(&Selection::UkraineFlag.command(75, Some(45)))
                .await;
            let trident = link
                .send(&Selection::UkraineTrident.command(75, Some(50)))
                .await;
            restore_test_lighting(&mut link, was_on, &settings).await;

            flag.expect("send Ukrainian flag");
            trident.expect("send Ukrainian trident");
            println!("The curtain acknowledged both Ukrainian animations.");
        });
    }

    async fn saved_test_lighting(link: &mut Link) -> (bool, Vec<u8>) {
        let before = link.query(1, 8).await.expect("read current lighting");
        let mut settings = before[4..].to_vec();
        match settings[0] {
            1 | 3 => {}
            2 if settings[1] < 200 => {
                let brightness = u16::from_be_bytes([settings[2], settings[3]]) * 10;
                settings[2..4].copy_from_slice(&brightness.to_be_bytes());
            }
            _ => panic!("Current lighting mode cannot be restored: {before:?}"),
        }
        (before[3] == 1, settings)
    }

    async fn restore_test_lighting(link: &mut Link, was_on: bool, settings: &[u8]) {
        link.write_and_confirm(protocol::packet(3, settings))
            .await
            .expect("restore original lighting");
        if !was_on {
            link.write_and_confirm(protocol::power(false))
                .await
                .expect("restore power");
        }
    }
}
