# Hypelights

A Rust desktop controller for the BESL18P Bluetooth curtain lights.

## Run on Windows

Open `target/x86_64-pc-windows-gnu/release/hypelights.exe` in Windows. With the curtain powered and nearby, enter its Bluetooth address and click **Connect**. Disconnect the Hello Fairy phone app if it already has the curtain connected.

Use **Turn on** and **Turn off** for power. Choose a solid color or a built-in scene, set the brightness, and click **Apply lighting**. Applying lighting also turns the curtain on. Scene numbers identify effects stored on the curtain. The preview shows the selected solid color.

## Build

Run `cargo build --locked --release` in Windows and open `target/release/hypelights.exe`. From WSL with the Windows GNU Rust target and MinGW installed, use the following command.

```sh
cargo build --locked --release --target x86_64-pc-windows-gnu
```

For a UI preview without Bluetooth, pass `--demo`. Run `cargo test --locked` for packet encoding checks. On Windows, set `HYPELIGHTS_DEVICE_ADDRESS` to your curtain’s Bluetooth address before running `cargo test --locked connects_to_curtain -- --ignored` for a live smoke test.

[BESL18 manual](https://manuals.plus/m/9acf32f46e339cbe3f50f70a0399146bb3d29df07d0cb0a53408c39e2f9eb1ca_optim.pdf) · [Hello Fairy protocol reference](https://github.com/nzrutman/hello_fairy_ble)

## Scenes and artwork

**Built-in scenes** lists the named and numbered effects supported by the curtain. Select a tile, set **Scene speed**, then click **Apply lighting**. Scenes 58–63 have scrolling previews. Presets without local artwork can be viewed on the curtain itself.

A personal checkout can use locally sourced Hello Fairy previews and its bitmap font via `assets/scene-previews.json` and `assets/scene-text-font.bin`. The catalog expects an array of `id`, `name`, `category`, and `frames` entries. Each frame is a 20 × 20 row-major RGB image encoded as hex. The optional font contains 16 one-byte rows per ASCII glyph, starting with space. These files are ignored by Git. A local `assets/device-address.txt` can prefill the Bluetooth address when building.

**Ukrainian Flag** and **Ukrainian Trident** are included as 20 × 20 scenes at the top of the grid. The flag waves and a flash of light crosses the trident. Both loop on the curtain and use speed and brightness controls. The Tryzub bitmap adapts [the public-domain Ukrainian trident outline](https://commons.wikimedia.org/wiki/File:%D0%A2%D1%80%D0%B8%D0%B7%D1%83%D0%B1.svg).

On each connection, Hypelights refreshes the reported preset settings and stores the catalog IDs and settings in `%LOCALAPPDATA%\Hypelights\presets.json`.

In **Studio**, paint the 20 × 20 grid. Use **Add** to insert a blank frame or **Duplicate** to start from the current one. Choose **Preview animation** to play the frames in the app, and name and save your work for reuse.

- A single frame uploads as artwork and can use the curtain's movement effects.
- Two frames upload together. The curtain loops them using **Blink** or **Breathe**, with its own speed control.
- Three or more frames loop from the PC while Hypelights stays connected. Each frame takes several seconds to transfer, plus the selected hold time. **Stop curtain playback** leaves the last completed frame displayed.

Studio includes **Pink Flamingo**, **Flamingo text**, and **Random RGB** starters. The flamingo alternates between two pink poses. The white “PINK FLAMINGO” marquee scrolls on the curtain with its own speed slider. Random RGB generates a fresh pair of random hue patterns across all 400 LEDs each time it is selected. The curtain's custom-art palette represents saturated hues, white, and off.

The local artwork library is `%LOCALAPPDATA%\Hypelights\artwork.json`. Each frame contains 400 row-major pixel values. Values 0–180 encode hue in 2° steps, 254 is off, and 255 is white. Existing saved artwork opens as a one-frame design.

[Hello Fairy app](https://hellofairy.com/) · [Two-picture protocol](https://github.com/nzrutman/hello_fairy_ble/blob/main/PROTOCOL.md#two-picture-playback)

## Music

The **Music** tab selects one of four effects that use the curtain's built-in microphone. After selection the curtain processes nearby sound on its own. Hypelights sends the effect, sensitivity, brightness, and color settings. In the Hello Fairy phone-microphone mode, the phone instead analyzes its microphone signal and sends updates over Bluetooth. Hypelights currently uses the curtain microphone.

[Picture protocol](https://github.com/nzrutman/hello_fairy_ble/blob/main/PROTOCOL.md) · [Music protocol](https://github.com/nzrutman/hello_fairy_ble/blob/main/NATIVE_EVIDENCE.md#music-and-preset-speed)


## License

Hypelights source code is released under the MIT License. See [LICENSE](LICENSE). Optional Hello Fairy artwork and font data are supplied locally under their respective terms.
