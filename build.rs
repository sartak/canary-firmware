use serde::Deserialize;
use std::{collections::HashMap, env, fs, path::Path};

#[derive(Deserialize)]
struct Config {
    usb: UsbConfig,
    split: SplitConfig,
    keyboard: KeyboardConfig,
    keys: KeysConfig,
}

#[derive(Deserialize)]
struct KeyboardConfig {
    layers: Vec<String>,
}

#[derive(Deserialize)]
struct KeysConfig {
    left: HashMap<String, KeyConfig>,
    right: HashMap<String, KeyConfig>,
}

#[derive(Deserialize)]
struct KeyConfig {
    id: String,
    tap: HashMap<String, String>,
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

fn parse_keycode(tap: &str) -> Result<String, String> {
    // Single lowercase letter -> uppercase variant
    if tap.len() == 1 {
        let c = tap.chars().next().unwrap();
        if c.is_ascii_lowercase() {
            return Ok(c.to_ascii_uppercase().to_string());
        }
    }
    // Punctuation and special keys
    match tap {
        "," => Ok("Comma".into()),
        "." => Ok("Period".into()),
        "'" => Ok("Apostrophe".into()),
        "Space" | "Enter" | "Backspace" | "Tab" | "Dup" => Ok(tap.into()),
        _ => Err(format!("invalid tap value: {:?}", tap)),
    }
}

fn parse_pin_id(s: &str) -> Result<u8, String> {
    s.strip_prefix("pin_")
        .ok_or_else(|| format!("key must start with 'pin_': {:?}", s))?
        .parse::<u8>()
        .map_err(|_| format!("invalid pin id: {:?}", s))
}

fn parse_keys(keys: &KeysConfig, layers: &[String]) -> Result<Vec<u8>, String> {
    let mut left_pins: Vec<u8> = Vec::new();
    for (key_name, key_config) in &keys.left {
        let pin_id = parse_pin_id(key_name)?;
        for (layer, tap) in &key_config.tap {
            if !layers.contains(layer) {
                return Err(format!("left {}: unknown layer {:?}", key_name, layer));
            }
            parse_keycode(tap).map_err(|e| format!("left {} {}: {}", key_name, layer, e))?;
        }
        left_pins.push(pin_id);
    }
    left_pins.sort();

    let mut right_pins: Vec<u8> = Vec::new();
    for (key_name, key_config) in &keys.right {
        let pin_id = parse_pin_id(key_name)?;
        for (layer, tap) in &key_config.tap {
            if !layers.contains(layer) {
                return Err(format!("right {}: unknown layer {:?}", key_name, layer));
            }
            parse_keycode(tap).map_err(|e| format!("right {} {}: {}", key_name, layer, e))?;
        }
        right_pins.push(pin_id);
    }
    right_pins.sort();

    if left_pins != right_pins {
        return Err(format!(
            "left and right must have same pins: left={:?}, right={:?}",
            left_pins, right_pins
        ));
    }

    Ok(left_pins)
}

fn main() {
    println!("cargo:rerun-if-changed=config.toml");

    let config_str = fs::read_to_string("config.toml").expect("Failed to read config.toml");
    let config: Config = toml::from_str(&config_str).expect("Failed to parse config.toml");

    let gpio_pins =
        parse_keys(&config.keys, &config.keyboard.layers).expect("Invalid key configuration");
    let num_keys = gpio_pins.len();

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("config.rs");

    // Build lookup from GPIO pin -> config
    let left_by_pin: HashMap<u8, &KeyConfig> = config
        .keys
        .left
        .iter()
        .map(|(name, cfg)| (parse_pin_id(name).unwrap(), cfg))
        .collect();
    let right_by_pin: HashMap<u8, &KeyConfig> = config
        .keys
        .right
        .iter()
        .map(|(name, cfg)| (parse_pin_id(name).unwrap(), cfg))
        .collect();

    let layers = &config.keyboard.layers;
    let num_layers = layers.len();

    // Generate tap array for a key config
    let gen_tap_array = |cfg: &KeyConfig| -> String {
        let taps: Vec<String> = layers
            .iter()
            .map(|layer| {
                cfg.tap
                    .get(layer)
                    .map(|tap| format!("Some(Keycode::{})", parse_keycode(tap).unwrap()))
                    .unwrap_or_else(|| "None".into())
            })
            .collect();
        format!("[{}]", taps.join(", "))
    };

    // Generate arrays indexed by scanner index (position in gpio_pins)
    let left_entries: Vec<String> = gpio_pins
        .iter()
        .map(|&gpio| {
            let cfg = left_by_pin.get(&gpio).unwrap();
            format!(
                "KeyConfig {{ id: {:?}, tap: {} }}",
                cfg.id,
                gen_tap_array(cfg)
            )
        })
        .collect();
    let right_entries: Vec<String> = gpio_pins
        .iter()
        .map(|&gpio| {
            let cfg = right_by_pin.get(&gpio).unwrap();
            format!(
                "KeyConfig {{ id: {:?}, tap: {} }}",
                cfg.id,
                gen_tap_array(cfg)
            )
        })
        .collect();

    let scanner_pins: Vec<String> = gpio_pins
        .iter()
        .enumerate()
        .map(|(idx, gpio)| format!("Keypin::new($p.PIN_{}, {})", gpio, idx))
        .collect();

    let generated = format!(
        r#"use crate::key::Keycode;

pub const USB_VENDOR_ID: u16 = {:#06x};
pub const USB_PRODUCT_ID: u16 = {:#06x};
pub const USB_MANUFACTURER: &str = {:?};
pub const USB_PRODUCT: &str = {:?};
pub const USB_SERIAL_NUMBER: &str = {:?};

pub const SPLIT_BIT_RATE: u64 = {};

pub const NUM_LAYERS: usize = {num_layers};

pub struct KeyConfig {{
    pub id: &'static str,
    pub tap: [Option<Keycode>; NUM_LAYERS],
}}

pub const NUM_KEYS: usize = {num_keys};
pub const LEFT_KEYS: [KeyConfig; NUM_KEYS] = [{left_keys}];
pub const RIGHT_KEYS: [KeyConfig; NUM_KEYS] = [{right_keys}];

macro_rules! keypins {{
    ($p:expr) => {{
        [{scanner_pins}]
    }};
}}
pub(crate) use keypins;
"#,
        config.usb.vendor_id,
        config.usb.product_id,
        config.usb.manufacturer,
        config.usb.product,
        config.usb.serial_number,
        config.split.bit_rate,
        num_layers = num_layers,
        num_keys = num_keys,
        left_keys = left_entries.join(", "),
        right_keys = right_entries.join(", "),
        scanner_pins = scanner_pins.join(", "),
    );

    fs::write(&dest_path, generated).expect("Failed to write config.rs");
}
