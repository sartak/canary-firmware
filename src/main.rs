#![no_std]
#![no_main]

mod keypin;
mod matrix;

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State as AcmState};
use embassy_usb::class::hid::{HidReaderWriter, State as HidState};
use embassy_usb::{Builder, Config};
use futures_util::StreamExt;
use keypin::Keypin;
use matrix::{Matrix, MatrixEvent};
use panic_halt as _;
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

    let driver = Driver::new(p.USB, Irqs);

    static CONFIG_DESCRIPTOR: StaticCell<[u8; USB_DESCRIPTOR_BUF_SIZE]> = StaticCell::new();
    static BOS_DESCRIPTOR: StaticCell<[u8; USB_DESCRIPTOR_BUF_SIZE]> = StaticCell::new();
    static MSOS_DESCRIPTOR: StaticCell<[u8; USB_DESCRIPTOR_BUF_SIZE]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; USB_MAX_PACKET_SIZE]> = StaticCell::new();
    static ACM_STATE: StaticCell<AcmState> = StaticCell::new();
    static KEYBOARD_HID_STATE: StaticCell<HidState> = StaticCell::new();
    static MOUSE_HID_STATE: StaticCell<HidState> = StaticCell::new();

    let mut config = Config::new(0x2E8A, 0x000a);
    config.manufacturer = Some("shawn.dev");
    config.product = Some("Sweep");
    config.serial_number = Some("1");
    config.max_power = USB_MAX_POWER;
    config.max_packet_size_0 = USB_MAX_PACKET_SIZE as u8;
    config.device_class = 0xef;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;
    config.composite_with_iads = true;

    let mut builder = Builder::new(
        driver,
        config,
        CONFIG_DESCRIPTOR.init([0; USB_DESCRIPTOR_BUF_SIZE]),
        BOS_DESCRIPTOR.init([0; USB_DESCRIPTOR_BUF_SIZE]),
        MSOS_DESCRIPTOR.init([0; USB_DESCRIPTOR_BUF_SIZE]),
        CONTROL_BUF.init([0; USB_MAX_PACKET_SIZE]),
    );

    let mut serial = CdcAcmClass::new(
        &mut builder,
        ACM_STATE.init(AcmState::new()),
        USB_MAX_PACKET_SIZE as u16,
    );

    let _keyboard = HidReaderWriter::<_, 1, KEYBOARD_MAX_PACKET_SIZE>::new(
        &mut builder,
        KEYBOARD_HID_STATE.init(HidState::new()),
        embassy_usb::class::hid::Config {
            report_descriptor: KeyboardReport::desc(),
            request_handler: None,
            poll_ms: HID_POLL_MS,
            max_packet_size: KEYBOARD_MAX_PACKET_SIZE as u16,
        },
    );

    let _mouse = HidReaderWriter::<_, 1, MOUSE_MAX_PACKET_SIZE>::new(
        &mut builder,
        MOUSE_HID_STATE.init(HidState::new()),
        embassy_usb::class::hid::Config {
            report_descriptor: MouseReport::desc(),
            request_handler: None,
            poll_ms: HID_POLL_MS,
            max_packet_size: MOUSE_MAX_PACKET_SIZE as u16,
        },
    );

    let mut usb = builder.build();
    let usb = usb.run();

    let mut matrix = Matrix::new([
        Keypin::new(p.PIN_0, "0"),
        // 1 is used for UART
        Keypin::new(p.PIN_2, "2"),
        Keypin::new(p.PIN_3, "3"),
        Keypin::new(p.PIN_4, "4"),
        Keypin::new(p.PIN_5, "5"),
        Keypin::new(p.PIN_6, "6"),
        Keypin::new(p.PIN_7, "7"),
        Keypin::new(p.PIN_8, "8"),
        Keypin::new(p.PIN_9, "9"),
        // 10 is not broken out in Pro Micro form factor
        Keypin::new(p.PIN_10, "10"),
        // 11 is not broken out in Pro Micro form factor
        Keypin::new(p.PIN_11, "11"),
        Keypin::new(p.PIN_12, "12"),
        Keypin::new(p.PIN_13, "13"),
        Keypin::new(p.PIN_14, "14"),
        Keypin::new(p.PIN_15, "15"),
        Keypin::new(p.PIN_16, "16"),
        // 17 is not broken out in Pro Micro form factor
        Keypin::new(p.PIN_17, "17"),
        // 18 is not broken out in Pro Micro form factor
        Keypin::new(p.PIN_18, "18"),
        // 19 is not broken out in Pro Micro form factor
        Keypin::new(p.PIN_19, "19"),
        Keypin::new(p.PIN_20, "20"),
        Keypin::new(p.PIN_21, "21"),
        Keypin::new(p.PIN_22, "22"),
        Keypin::new(p.PIN_23, "23"),
        // 24 is not broken out in Pro Micro form factor
        Keypin::new(p.PIN_24, "24"),
        Keypin::new(p.PIN_25, "25"),
        Keypin::new(p.PIN_26, "26"),
        Keypin::new(p.PIN_27, "27"),
        Keypin::new(p.PIN_28, "28"),
        Keypin::new(p.PIN_29, "29"),
    ]);

    let keyboard = async {
        loop {
            if let Some(event) = matrix.next().await {
                match event {
                    MatrixEvent::KeyDown(label) => {
                        let _ = SERIAL_CHANNEL.try_send(label);
                        let _ = SERIAL_CHANNEL.try_send(" down\r\n");
                    }
                    MatrixEvent::KeyUp(label) => {
                        let _ = SERIAL_CHANNEL.try_send(label);
                        let _ = SERIAL_CHANNEL.try_send(" up\r\n");
                    }
                }
            }
        }
    };

    let serial = async {
        loop {
            serial.wait_connection().await;

            // Drain any stale events
            while SERIAL_CHANNEL.try_receive().is_ok() {}

            loop {
                let msg = SERIAL_CHANNEL.receive().await;
                if serial.write_packet(msg.as_bytes()).await.is_err() {
                    break;
                }
            }
        }
    };

    embassy_futures::join::join3(usb, serial, keyboard).await;
}
