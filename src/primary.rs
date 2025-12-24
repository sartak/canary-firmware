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
use static_cell::StaticCell;
use usbd_hid::descriptor::{KeyboardReport, MouseReport, SerializedDescriptor};

use crate::config;
use crate::keypin::Keypin;
use crate::matrix::Matrix;
use crate::scanner::{ScanEvent, Scanner};
use crate::stash::{self, Stash};
use crate::sync;

const SERIAL_CHANNEL_CAPACITY: usize = 32;
const USB_MAX_PACKET_SIZE: usize = 64;
const USB_MAX_POWER: u16 = 50; // milliamps
const USB_DESCRIPTOR_BUF_SIZE: usize = 512;
const KEYBOARD_MAX_PACKET_SIZE: usize = 8;
const HID_POLL_MS: u8 = 1;
const MOUSE_MAX_PACKET_SIZE: usize = 5;

bind_interrupts!(struct UsbIrqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

static SERIAL_CHANNEL: Channel<ThreadModeRawMutex, &'static str, SERIAL_CHANNEL_CAPACITY> =
    Channel::new();

#[derive(Clone, Copy)]
enum KeyEvent {
    Down(stash::Hand, u8),
    Up(stash::Hand, u8),
}

static KEY_CHANNEL: Channel<ThreadModeRawMutex, KeyEvent, SERIAL_CHANNEL_CAPACITY> = Channel::new();
static KEYBOARD_REPORT_CHANNEL: Channel<ThreadModeRawMutex, KeyboardReport, 4> = Channel::new();

fn pin_key(hand: stash::Hand, index: u8) -> &'static config::KeyConfig {
    match hand {
        stash::Hand::Left => &config::LEFT_KEYS[index as usize],
        stash::Hand::Right => &config::RIGHT_KEYS[index as usize],
    }
}

pub async fn run(p: embassy_rp::Peripherals) {
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

    let local_hand = config.hand;

    match local_hand {
        stash::Hand::Left => {
            let _ = SERIAL_CHANNEL.try_send("Configured as left-handed\r\n");
        }
        stash::Hand::Right => {
            let _ = SERIAL_CHANNEL.try_send("Configured as right-handed\r\n");
        }
    }

    let _ = SERIAL_CHANNEL.try_send("Role: Primary (USB connected)\r\n");

    let driver = Driver::new(p.USB, UsbIrqs);

    static CONFIG_DESCRIPTOR: StaticCell<[u8; USB_DESCRIPTOR_BUF_SIZE]> = StaticCell::new();
    static BOS_DESCRIPTOR: StaticCell<[u8; USB_DESCRIPTOR_BUF_SIZE]> = StaticCell::new();
    static MSOS_DESCRIPTOR: StaticCell<[u8; USB_DESCRIPTOR_BUF_SIZE]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; USB_MAX_PACKET_SIZE]> = StaticCell::new();
    static ACM_STATE: StaticCell<AcmState> = StaticCell::new();
    static KEYBOARD_HID_STATE: StaticCell<HidState> = StaticCell::new();
    static MOUSE_HID_STATE: StaticCell<HidState> = StaticCell::new();

    let mut usb_config = UsbConfig::new(config::USB_VENDOR_ID, config::USB_PRODUCT_ID);
    usb_config.manufacturer = Some(config::USB_MANUFACTURER);
    usb_config.product = Some(config::USB_PRODUCT);
    usb_config.serial_number = Some(config::USB_SERIAL_NUMBER);
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

    let keyboard_hid = HidReaderWriter::<_, 1, KEYBOARD_MAX_PACKET_SIZE>::new(
        &mut builder,
        KEYBOARD_HID_STATE.init(HidState::new()),
        HidConfig {
            report_descriptor: KeyboardReport::desc(),
            request_handler: None,
            poll_ms: HID_POLL_MS,
            max_packet_size: KEYBOARD_MAX_PACKET_SIZE as u16,
        },
    );
    let (mut keyboard_reader, mut keyboard_writer) = keyboard_hid.split();

    let mouse_hid = HidReaderWriter::<_, 1, MOUSE_MAX_PACKET_SIZE>::new(
        &mut builder,
        MOUSE_HID_STATE.init(HidState::new()),
        HidConfig {
            report_descriptor: MouseReport::desc(),
            request_handler: None,
            poll_ms: HID_POLL_MS,
            max_packet_size: MOUSE_MAX_PACKET_SIZE as u16,
        },
    );
    let (mut mouse_reader, _mouse_writer) = mouse_hid.split();

    let mut usb = builder.build();
    let usb = usb.run();

    let mut scanner = Scanner::new(config::keypins!(p));

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

    let scan_keys = async {
        loop {
            if let Some(event) = scanner.next().await {
                let key_event = match event {
                    ScanEvent::Down(index) => KeyEvent::Down(local_hand, index),
                    ScanEvent::Up(index) => KeyEvent::Up(local_hand, index),
                };
                let _ = KEY_CHANNEL.try_send(key_event);
            }
        }
    };

    let sync_recv = async {
        let mut sync_rx = sync::SyncReceiver::new(p.PIO0, p.PIN_1);
        let remote_hand = local_hand.opposite();
        let mut prev_state: u32 = 0;

        loop {
            // recv returns None if Manchester decoding fails (noise/corruption)
            let Some(state) = sync_rx.recv().await else {
                let _ = SERIAL_CHANNEL.try_send("Sync read error\r\n");
                continue;
            };
            let changed = state ^ prev_state;
            for i in 0..17u8 {
                let mask = 1 << i;
                if changed & mask != 0 {
                    if state & mask != 0 {
                        let _ = KEY_CHANNEL.try_send(KeyEvent::Down(remote_hand, i));
                    } else {
                        let _ = KEY_CHANNEL.try_send(KeyEvent::Up(remote_hand, i));
                    }
                }
            }
            prev_state = state;
        }
    };

    let process_keys = async {
        let mut matrix = Matrix::default();

        loop {
            let event = KEY_CHANNEL.receive().await;
            match event {
                KeyEvent::Down(hand, index) => {
                    matrix.set_down(hand, index);
                    let _ = SERIAL_CHANNEL.try_send(pin_key(hand, index).id);
                    let _ = SERIAL_CHANNEL.try_send(" down\r\n");
                }
                KeyEvent::Up(hand, index) => {
                    matrix.set_up(hand, index);
                    let _ = SERIAL_CHANNEL.try_send(pin_key(hand, index).id);
                    let _ = SERIAL_CHANNEL.try_send(" up\r\n");
                }
            }
            let _ = KEYBOARD_REPORT_CHANNEL.try_send(matrix.keyboard_report());
        }
    };

    let hid_keyboard_rx = async {
        let mut buf = [0u8; KEYBOARD_MAX_PACKET_SIZE];
        loop {
            let _ = keyboard_reader.read(&mut buf).await;
        }
    };

    let hid_keyboard_tx = async {
        loop {
            let report = KEYBOARD_REPORT_CHANNEL.receive().await;
            let _ = keyboard_writer.write_serialize(&report).await;
        }
    };

    let hid_mouse = async {
        let mut buf = [0u8; MOUSE_MAX_PACKET_SIZE];
        loop {
            let _ = mouse_reader.read(&mut buf).await;
        }
    };

    embassy_futures::join::join4(
        embassy_futures::join::join3(usb, serial_tx, serial_rx),
        embassy_futures::join::join(scan_keys, sync_recv),
        embassy_futures::join::join3(hid_keyboard_rx, hid_keyboard_tx, hid_mouse),
        process_keys,
    )
    .await;
}
