use std::{fs, path::Path};

fn main() {
    for (file, configuration) in [
        ("assets/scene-previews.json", "hypelights_vendor_previews"),
        ("assets/scene-text-font.bin", "hypelights_vendor_font"),
    ] {
        println!("cargo:rerun-if-changed={file}");
        println!("cargo:rustc-check-cfg=cfg({configuration})");
        if Path::new(file).is_file() {
            println!("cargo:rustc-cfg={configuration}");
        }
    }

    let file = "assets/device-address.txt";
    println!("cargo:rerun-if-changed={file}");
    if let Ok(contents) = fs::read_to_string(file) {
        let address = contents.trim();
        if address.len() == 17
            && address
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b':')
        {
            println!("cargo:rustc-env=HYPELIGHTS_LOCAL_ADDRESS={address}");
        }
    }
}
