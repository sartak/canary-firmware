#![no_std]
#![no_main]

mod debounce;
mod keypin;
mod matrix;
mod stash;

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_rp::watchdog::Watchdog;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Timer;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State as AcmState};
use embassy_usb::class::hid::{Config as HidConfig, HidReaderWriter, State as HidState};
use embassy_usb::{Builder, Config as UsbConfig};
use futures_util::StreamExt;
use keypin::Keypin;
use matrix::{Matrix, MatrixEvent};
use panic_halt as _;
use stash::Stash;
use static_cell::StaticCell;
use usbd_hid::descriptor::{KeyboardReport, MouseReport, SerializedDescriptor};

const SERIAL_CHANNEL_CAPACITY: usize = 8;
const USB_MAX_PACKET_SIZE: usize = 64;
const USB_MAX_POWER: u16 = 50; // milliamps
const USB_DESCRIPTOR_BUF_SIZE: usize = 512;
const KEYBOARD_MAX_PACKET_SIZE: usize = 8;
const HID_POLL_MS: u8 = 1;
const MOUSE_MAX_PACKET_SIZE: usize = 5;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

static SERIAL_CHANNEL: Channel<ThreadModeRawMutex, &'static str, SERIAL_CHANNEL_CAPACITY> =
    Channel::new();

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let mut stash = Stash::new(p.FLASH);
    let config = match stash.load() {
        Ok(c) => c,
        Err(e) => {
            let _ = SERIAL_CHANNEL.try_send("Failed to load config: ");
            let _ = SERIAL_CHANNEL.try_send(e);
            let _ = SERIAL_CHANNEL.try_send("\r\n");
            stash::Config::default()
        }
    };

    match config.hand {
        stash::Hand::Left => {
            let _ = SERIAL_CHANNEL.try_send("Configured as left-handed\r\n");
        }
        stash::Hand::Right => {
            let _ = SERIAL_CHANNEL.try_send("Configured as right-handed\r\n");
        }
    }

    let driver = Driver::new(p.USB, Irqs);

    static CONFIG_DESCRIPTOR: StaticCell<[u8; USB_DESCRIPTOR_BUF_SIZE]> = StaticCell::new();
    static BOS_DESCRIPTOR: StaticCell<[u8; USB_DESCRIPTOR_BUF_SIZE]> = StaticCell::new();
    static MSOS_DESCRIPTOR: StaticCell<[u8; USB_DESCRIPTOR_BUF_SIZE]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; USB_MAX_PACKET_SIZE]> = StaticCell::new();
    static ACM_STATE: StaticCell<AcmState> = StaticCell::new();
    static KEYBOARD_HID_STATE: StaticCell<HidState> = StaticCell::new();
    static MOUSE_HID_STATE: StaticCell<HidState> = StaticCell::new();

    let mut usb_config = UsbConfig::new(0x2E8A, 0x000a);
    usb_config.manufacturer = Some("shawn.dev");
    usb_config.product = Some("Sweep");
    usb_config.serial_number = Some("1");
    usb_config.max_power = USB_MAX_POWER;
    usb_config.max_packet_size_0 = USB_MAX_PACKET_SIZE as u8;
    usb_config.device_class = 0xef;
    usb_config.device_sub_class = 0x02;
    usb_config.device_protocol = 0x01;
    usb_config.composite_with_iads = true;

    let mut builder = Builder::new(
        driver,
        usb_config,
        CONFIG_DESCRIPTOR.init([0; USB_DESCRIPTOR_BUF_SIZE]),
        BOS_DESCRIPTOR.init([0; USB_DESCRIPTOR_BUF_SIZE]),
        MSOS_DESCRIPTOR.init([0; USB_DESCRIPTOR_BUF_SIZE]),
        CONTROL_BUF.init([0; USB_MAX_PACKET_SIZE]),
    );

    let serial = CdcAcmClass::new(
        &mut builder,
        ACM_STATE.init(AcmState::new()),
        USB_MAX_PACKET_SIZE as u16,
    );
    let (mut serial_writer, mut serial_reader) = serial.split();

    let keyboard = HidReaderWriter::<_, 1, KEYBOARD_MAX_PACKET_SIZE>::new(
        &mut builder,
        KEYBOARD_HID_STATE.init(HidState::new()),
        HidConfig {
            report_descriptor: KeyboardReport::desc(),
            request_handler: None,
            poll_ms: HID_POLL_MS,
            max_packet_size: KEYBOARD_MAX_PACKET_SIZE as u16,
        },
    );

    let _mouse = HidReaderWriter::<_, 1, MOUSE_MAX_PACKET_SIZE>::new(
        &mut builder,
        MOUSE_HID_STATE.init(HidState::new()),
        HidConfig {
            report_descriptor: MouseReport::desc(),
            request_handler: None,
            poll_ms: HID_POLL_MS,
            max_packet_size: MOUSE_MAX_PACKET_SIZE as u16,
        },
    );

    let mut usb = builder.build();
    let usb = usb.run();

    let mut matrix = match config.hand {
        stash::Hand::Left => {
            Matrix::new(
                config.hand,
                [
                    Keypin::new(p.PIN_0, "0", Some('g')),
                    // 1 is used for UART
                    Keypin::new(p.PIN_2, "2", Some('q')),
                    Keypin::new(p.PIN_3, "3", Some('j')),
                    Keypin::new(p.PIN_4, "4", Some('v')),
                    Keypin::new(p.PIN_5, "5", Some('d')),
                    Keypin::new(p.PIN_6, "6", Some('k')),
                    Keypin::new(p.PIN_7, "7", Some('w')),
                    Keypin::new(p.PIN_8, "8", None),
                    Keypin::new(p.PIN_9, "9", Some('\x08')),
                    // 10 is not broken out in Pro Micro form factor
                    Keypin::new(p.PIN_10, "10", None),
                    // 11 is not broken out in Pro Micro form factor
                    Keypin::new(p.PIN_11, "11", None),
                    Keypin::new(p.PIN_12, "12", None),
                    Keypin::new(p.PIN_13, "13", None),
                    Keypin::new(p.PIN_14, "14", None),
                    Keypin::new(p.PIN_15, "15", None),
                    Keypin::new(p.PIN_16, "16", None),
                    // 17 is not broken out in Pro Micro form factor
                    Keypin::new(p.PIN_17, "17", None),
                    // 18 is not broken out in Pro Micro form factor
                    Keypin::new(p.PIN_18, "18", None),
                    // 19 is not broken out in Pro Micro form factor
                    Keypin::new(p.PIN_19, "19", None),
                    Keypin::new(p.PIN_20, "20", Some('r')),
                    Keypin::new(p.PIN_21, "21", Some('t')),
                    Keypin::new(p.PIN_22, "22", Some('c')),
                    Keypin::new(p.PIN_23, "23", Some('s')),
                    // 24 is not broken out in Pro Micro form factor
                    Keypin::new(p.PIN_24, "24", None),
                    Keypin::new(p.PIN_25, "25", None),
                    Keypin::new(p.PIN_26, "26", Some('l')),
                    Keypin::new(p.PIN_27, "27", Some('y')),
                    Keypin::new(p.PIN_28, "28", Some('p')),
                    Keypin::new(p.PIN_29, "29", Some('b')),
                ],
            )
        }
        stash::Hand::Right => {
            Matrix::new(
                config.hand,
                [
                    Keypin::new(p.PIN_0, "0", Some('m')),
                    // 1 is used for UART
                    Keypin::new(p.PIN_2, "2", Some('\n')),
                    Keypin::new(p.PIN_3, "3", Some(',')),
                    Keypin::new(p.PIN_4, "4", Some('.')),
                    Keypin::new(p.PIN_5, "5", Some('h')),
                    Keypin::new(p.PIN_6, "6", Some('f')),
                    Keypin::new(p.PIN_7, "7", Some('\'')),
                    Keypin::new(p.PIN_8, "8", None),
                    Keypin::new(p.PIN_9, "9", Some(' ')),
                    // 10 is not broken out in Pro Micro form factor
                    Keypin::new(p.PIN_10, "10", None),
                    // 11 is not broken out in Pro Micro form factor
                    Keypin::new(p.PIN_11, "11", None),
                    Keypin::new(p.PIN_12, "12", None),
                    Keypin::new(p.PIN_13, "13", None),
                    Keypin::new(p.PIN_14, "14", None),
                    Keypin::new(p.PIN_15, "15", None),
                    Keypin::new(p.PIN_16, "16", None),
                    // 17 is not broken out in Pro Micro form factor
                    Keypin::new(p.PIN_17, "17", None),
                    // 18 is not broken out in Pro Micro form factor
                    Keypin::new(p.PIN_18, "18", None),
                    // 19 is not broken out in Pro Micro form factor
                    Keypin::new(p.PIN_19, "19", None),
                    Keypin::new(p.PIN_20, "20", Some('i')),
                    Keypin::new(p.PIN_21, "21", Some('n')),
                    Keypin::new(p.PIN_22, "22", Some('a')),
                    Keypin::new(p.PIN_23, "23", Some('e')),
                    // 24 is not broken out in Pro Micro form factor
                    Keypin::new(p.PIN_24, "24", None),
                    Keypin::new(p.PIN_25, "25", None),
                    Keypin::new(p.PIN_26, "26", Some('u')),
                    Keypin::new(p.PIN_27, "27", Some('o')),
                    Keypin::new(p.PIN_28, "28", Some('f')),
                    Keypin::new(p.PIN_29, "29", Some('z')),
                ],
            )
        }
    };

    let (_, mut writer) = keyboard.split();

    let keyboard = async {
        loop {
            if let Some(event) = matrix.next().await {
                match event {
                    MatrixEvent::KeyDown(label, keycode) => {
                        let _ = SERIAL_CHANNEL.try_send(if config.hand == stash::Hand::Left {
                            "Left "
                        } else {
                            "Right "
                        });
                        let _ = SERIAL_CHANNEL.try_send(label);
                        let _ = SERIAL_CHANNEL.try_send(" down\r\n");

                        if let Some(keycode) = keycode {
                            let hid_keycode = match keycode {
                                'a'..='z' => (keycode as u8) - b'a' + 0x04,
                                'A'..='Z' => (keycode as u8) - b'A' + 0x04,
                                '\n' => 0x28,
                                '\x08' => 0x2a,
                                ' ' => 0x2c,
                                '\'' => 0x34,
                                ',' => 0x36,
                                '.' => 0x37,
                                _ => 0,
                            };
                            let report = KeyboardReport {
                                modifier: 0,
                                reserved: 0,
                                leds: 0,
                                keycodes: [hid_keycode, 0, 0, 0, 0, 0],
                            };
                            let _ = writer.write_serialize(&report).await;
                        }
                    }
                    MatrixEvent::KeyUp(label, keycode) => {
                        let _ = SERIAL_CHANNEL.try_send(if config.hand == stash::Hand::Left {
                            "Left "
                        } else {
                            "Right "
                        });
                        let _ = SERIAL_CHANNEL.try_send(label);
                        let _ = SERIAL_CHANNEL.try_send(" up\r\n");

                        if keycode.is_some() {
                            let report = KeyboardReport {
                                modifier: 0,
                                reserved: 0,
                                leds: 0,
                                keycodes: [0, 0, 0, 0, 0, 0],
                            };
                            let _ = writer.write_serialize(&report).await;
                        }
                    }
                }
            }
        }
    };

    let serial_tx = async {
        loop {
            Timer::after_millis(1000).await;
            serial_writer.wait_connection().await;

            loop {
                let msg = SERIAL_CHANNEL.receive().await;
                if serial_writer.write_packet(msg.as_bytes()).await.is_err() {
                    break;
                }
            }
        }
    };

    let mut watchdog = Watchdog::new(p.WATCHDOG);

    let serial_rx = async {
        let mut buf = [0u8; USB_MAX_PACKET_SIZE];
        loop {
            serial_reader.wait_connection().await;

            loop {
                match serial_reader.read_packet(&mut buf).await {
                    Ok(n) if n > 0 => match buf[0] {
                        b'L' => {
                            let mut config = config.clone();
                            config.hand = stash::Hand::Left;
                            if let Err(e) = stash.save(config) {
                                let _ = SERIAL_CHANNEL.try_send("Failed to save: ");
                                let _ = SERIAL_CHANNEL.try_send(e);
                                let _ = SERIAL_CHANNEL.try_send("\r\n");
                            } else {
                                let _ =
                                    SERIAL_CHANNEL.try_send("Set hand to Left, rebooting...\r\n");
                                Timer::after_millis(100).await;
                                watchdog.trigger_reset();
                            }
                        }
                        b'R' => {
                            let mut config = config.clone();
                            config.hand = stash::Hand::Right;
                            if let Err(e) = stash.save(config) {
                                let _ = SERIAL_CHANNEL.try_send("Failed to save: ");
                                let _ = SERIAL_CHANNEL.try_send(e);
                                let _ = SERIAL_CHANNEL.try_send("\r\n");
                            } else {
                                let _ =
                                    SERIAL_CHANNEL.try_send("Set hand to Right, rebooting...\r\n");
                                Timer::after_millis(100).await;
                                watchdog.trigger_reset();
                            }
                        }
                        _ => {
                            let _ = SERIAL_CHANNEL.try_send("Unknown command\r\n");
                        }
                    },
                    Err(_) => break,
                    _ => {}
                }
            }
        }
    };

    embassy_futures::join::join4(usb, serial_tx, serial_rx, keyboard).await;
}
