#![no_std]
#![no_main]

mod debounce;
mod key;
mod keypin;
mod matrix;
mod primary;
mod scanner;
mod secondary;
mod stash;
mod sync;

mod config {
    include!(concat!(env!("OUT_DIR"), "/config.rs"));
}

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::InterruptHandler as PioInterruptHandler;
use panic_halt as _;

bind_interrupts!(pub struct Irqs {
    PIO0_IRQ_0 => PioInterruptHandler<PIO0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // Sea Picro has VBUS connected to GP19
    // Read directly via PAC to avoid consuming the pin peripheral
    let is_primary = (rp_pac::SIO.gpio_in(0).read() & (1 << 19)) != 0;

    if is_primary {
        primary::run(p).await;
    } else {
        secondary::run(p).await;
    }
}
