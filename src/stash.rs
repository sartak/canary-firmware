use embassy_rp::flash::{Blocking, Flash};
use embassy_rp::{Peri, peripherals::FLASH};

const XIP_BASE: u32 = 0x10000000;
const CONFIG_OFFSET: u32 = 0x001FF000;
const MAGIC: u32 = 0x1113_0001;
const FLASH_SIZE: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Left,
    Right,
}

#[derive(Clone)]
pub struct Config {
    pub hand: Hand,
}

impl Default for Config {
    fn default() -> Self {
        Self { hand: Hand::Left }
    }
}

pub struct Stash {
    flash: Flash<'static, FLASH, Blocking, FLASH_SIZE>,
}

#[repr(C)]
struct RawConfig {
    magic: u32,
    hand: u32,
    _reserved: [u32; 1022],
}

impl TryFrom<RawConfig> for Config {
    type Error = &'static str;
    fn try_from(raw: RawConfig) -> Result<Self, Self::Error> {
        if raw.magic != MAGIC {
            return Err("Invalid magic");
        }

        let hand = match raw.hand {
            0 => Hand::Left,
            1 => Hand::Right,
            _ => return Err("Invalid hand"),
        };

        Ok(Config { hand })
    }
}

impl TryFrom<Config> for RawConfig {
    type Error = &'static str;
    fn try_from(config: Config) -> Result<Self, Self::Error> {
        let hand = match config.hand {
            Hand::Left => 0,
            Hand::Right => 1,
        };

        Ok(RawConfig {
            magic: MAGIC,
            hand,
            _reserved: [0; 1022],
        })
    }
}

impl RawConfig {
    const fn size() -> usize {
        core::mem::size_of::<Self>()
    }
}

impl Stash {
    pub fn new(flash: Peri<'static, FLASH>) -> Self {
        Self {
            flash: Flash::new_blocking(flash),
        }
    }

    pub fn load(&self) -> Result<Config, &'static str> {
        let flash_ptr = (XIP_BASE + CONFIG_OFFSET) as *const RawConfig;
        // SAFETY: CONFIG_OFFSET points to valid flash memory that is readable
        // via XIP
        let raw_config = unsafe { core::ptr::read_volatile(flash_ptr) };

        Config::try_from(raw_config)
    }

    pub fn save(&mut self, config: Config) -> Result<(), &'static str> {
        let raw_config = RawConfig::try_from(config)?;

        // SAFETY: RawConfig is repr(C) with known size and alignment
        let config_bytes = unsafe {
            core::slice::from_raw_parts(
                &raw_config as *const RawConfig as *const u8,
                RawConfig::size(),
            )
        };

        self.flash
            .blocking_erase(CONFIG_OFFSET, CONFIG_OFFSET + RawConfig::size() as u32)
            .map_err(|_| "Flash erase failed")?;

        self.flash
            .blocking_write(CONFIG_OFFSET, config_bytes)
            .map_err(|_| "Flash write failed")?;

        Ok(())
    }
}
