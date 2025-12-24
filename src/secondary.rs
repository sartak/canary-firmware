use futures_util::StreamExt;

use crate::config;
use crate::keypin::Keypin;
use crate::scanner::{ScanEvent, Scanner};
use crate::sync;

pub async fn run(p: embassy_rp::Peripherals) {
    let mut sync_tx = sync::SyncSender::new(p.PIO0, p.PIN_1);
    let mut state: u32 = 0;

    let mut scanner = Scanner::new(config::keypins!(p));

    loop {
        if let Some(event) = scanner.next().await {
            match event {
                ScanEvent::Down(index) => state |= 1 << index,
                ScanEvent::Up(index) => state &= !(1 << index),
            }
            sync_tx.send(state).await;
        }
    }
}
