use serde::Deserialize;
use std::{env, fs, path::Path};

#[derive(Deserialize)]
struct Config {
    usb: UsbConfig,
    split: SplitConfig,
}

#[derive(Deserialize)]
struct UsbConfig {
    vendor_id: u16,
    product_id: u16,
    manufacturer: String,
    product: String,
    serial_number: String,
}

#[derive(Deserialize)]
struct SplitConfig {
    bit_rate: u64,
}

fn main() {
    println!("cargo:rerun-if-changed=config.toml");

    let config_str = fs::read_to_string("config.toml").expect("Failed to read config.toml");
    let config: Config = toml::from_str(&config_str).expect("Failed to parse config.toml");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("config.rs");

    let generated = format!(
        r#"pub const USB_VENDOR_ID: u16 = {:#06x};
pub const USB_PRODUCT_ID: u16 = {:#06x};
pub const USB_MANUFACTURER: &str = {:?};
pub const USB_PRODUCT: &str = {:?};
pub const USB_SERIAL_NUMBER: &str = {:?};

pub const SPLIT_BIT_RATE: u64 = {};
"#,
        config.usb.vendor_id,
        config.usb.product_id,
        config.usb.manufacturer,
        config.usb.product,
        config.usb.serial_number,
        config.split.bit_rate,
    );

    fs::write(&dest_path, generated).expect("Failed to write config.rs");
}
