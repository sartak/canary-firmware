#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State as AcmState};
use embassy_usb::{Builder, Config};
use panic_halt as _;
use static_cell::StaticCell;

const SERIAL_CHANNEL_CAPACITY: usize = 8;
const USB_MAX_PACKET_SIZE: usize = 64;
const USB_MAX_POWER: u16 = 50; // milliamps
const USB_DESCRIPTOR_BUF_SIZE: usize = 512;

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
    static CONTROL_BUF: StaticCell<[u8; USB_MAX_PACKET_SIZE]> = StaticCell::new();
    static ACM_STATE: StaticCell<AcmState> = StaticCell::new();

    let mut config = Config::new(0x2E8A, 0x000a);
    config.manufacturer = Some("shawn.dev");
    config.product = Some("Sweep");
    config.serial_number = Some("1");
    config.max_power = USB_MAX_POWER;
    config.max_packet_size_0 = USB_MAX_PACKET_SIZE as u8;

    let mut builder = Builder::new(
        driver,
        config,
        CONFIG_DESCRIPTOR.init([0; USB_DESCRIPTOR_BUF_SIZE]),
        BOS_DESCRIPTOR.init([0; USB_DESCRIPTOR_BUF_SIZE]),
        &mut [],
        CONTROL_BUF.init([0; USB_MAX_PACKET_SIZE]),
    );

    let mut serial = CdcAcmClass::new(
        &mut builder,
        ACM_STATE.init(AcmState::new()),
        USB_MAX_PACKET_SIZE as u16,
    );

    let mut usb = builder.build();
    let usb = usb.run();

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

    embassy_futures::join::join(usb, serial).await;
}
