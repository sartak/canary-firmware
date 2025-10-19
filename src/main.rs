#![no_std]
#![no_main]

use embassy_executor::Spawner;
use panic_halt as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let _p = embassy_rp::init(Default::default());

    loop {}
}
