use embassy_rp::Peri;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::PIN_1;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Timer;

const BIT_DELAY_NS: u64 = 3000000; // 3ms per bit (~2.7kbps, 30ms per byte)
const MAX_MESSAGE_LEN: usize = 2;

// Half-duplex split keyboard communication protocol:
// - Single wire on PIN_1, idle high with pull-up
// - Frame format per byte:
//   1. Sync pulse: low→high (receiver detects falling edge to resynchronize)
//   2. 8 data bits, MSB first
//   3. 1 even parity bit
// - 6μs per bit (~137 kbps)
// - Receiver samples at bit center after detecting sync pulse

#[derive(Debug, Clone, Copy)]
pub enum SyncMessage {
    Test(u8),
}

impl SyncMessage {
    fn msg_len(msg_type: u8) -> Option<usize> {
        match msg_type {
            1 => Some(1), // Test message: 1 byte (just the payload)
            _ => None,
        }
    }

    fn to_bytes(self) -> ([u8; MAX_MESSAGE_LEN], usize) {
        match self {
            SyncMessage::Test(val) => ([1, val], 2), // msg_type + payload
        }
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes.first()? {
            1 => Some(SyncMessage::Test(*bytes.get(1)?)),
            _ => None,
        }
    }
}

async fn receive_byte(pin: &mut Input<'_>) -> Result<u8, &'static str> {
    // Ensure we're in idle high state before looking for sync pulse
    while pin.is_low() {
        pin.wait_for_high().await;
    }

    // Wait for sync pulse falling edge
    pin.wait_for_low().await;
    let t0 = embassy_time::Instant::now();

    // Busy-wait for rising edge to get precise timing
    while pin.is_low() {}

    // Adaptive timing with async waits
    let target_ns = BIT_DELAY_NS * 2 + BIT_DELAY_NS / 2;
    let elapsed_ns = embassy_time::Instant::now().duration_since(t0).as_micros() * 1000;
    if target_ns > elapsed_ns {
        Timer::after_nanos(target_ns - elapsed_ns).await;
    }

    let mut byte = 0u8;
    let mut parity = 0u8;

    // Sample 8 data bits with adaptive async timing
    for i in 0..8 {
        let bit = if pin.is_high() { 1 } else { 0 };
        byte |= bit << (7 - i);
        parity ^= bit;

        // Adaptive async wait for next sample
        let next_sample_target_ns =
            BIT_DELAY_NS * 2 + BIT_DELAY_NS / 2 + BIT_DELAY_NS * (i as u64 + 1);
        let elapsed_ns = embassy_time::Instant::now().duration_since(t0).as_micros() * 1000;
        if next_sample_target_ns > elapsed_ns {
            Timer::after_nanos(next_sample_target_ns - elapsed_ns).await;
        }
    }

    // Sample parity bit
    let next_sample_target_ns = BIT_DELAY_NS * 2 + BIT_DELAY_NS / 2 + BIT_DELAY_NS * 8;
    let elapsed_ns = embassy_time::Instant::now().duration_since(t0).as_micros() * 1000;
    if next_sample_target_ns > elapsed_ns {
        Timer::after_nanos(next_sample_target_ns - elapsed_ns).await;
    }
    let parity_bit = if pin.is_high() { 1 } else { 0 };

    // Check parity
    if parity != parity_bit {
        return Err("parity check failed");
    }

    Ok(byte)
}

async fn read_sync_message(pin: &mut Input<'_>) -> Result<SyncMessage, &'static str> {
    // Read message type byte
    let msg_type = receive_byte(pin).await?;

    // Log what we got
    let _ = crate::SERIAL_CHANNEL.try_send("msg_type=0x");
    let hex_hi = (msg_type >> 4) & 0xF;
    let hex_lo = msg_type & 0xF;
    let _ = crate::SERIAL_CHANNEL.try_send(match hex_hi {
        0 => "0", 1 => "1", 2 => "2", 3 => "3", 4 => "4", 5 => "5", 6 => "6", 7 => "7",
        8 => "8", 9 => "9", 10 => "A", 11 => "B", 12 => "C", 13 => "D", 14 => "E", 15 => "F",
        _ => "?",
    });
    let _ = crate::SERIAL_CHANNEL.try_send(match hex_lo {
        0 => "0", 1 => "1", 2 => "2", 3 => "3", 4 => "4", 5 => "5", 6 => "6", 7 => "7",
        8 => "8", 9 => "9", 10 => "A", 11 => "B", 12 => "C", 13 => "D", 14 => "E", 15 => "F",
        _ => "?",
    });
    let _ = crate::SERIAL_CHANNEL.try_send(" ");

    // Determine how many more bytes to read
    let payload_len = SyncMessage::msg_len(msg_type).ok_or("unknown message type")?;

    // Read payload bytes
    let mut bytes = [0u8; MAX_MESSAGE_LEN];
    bytes[0] = msg_type;
    for i in 0..payload_len {
        bytes[i + 1] = receive_byte(pin).await?;
    }

    // Decode message
    SyncMessage::from_bytes(&bytes[..payload_len + 1]).ok_or("failed to decode message")
}

pub async fn primary(
    pin: Peri<'static, PIN_1>,
    rx_channel: &'static Channel<ThreadModeRawMutex, SyncMessage, 8>,
) {
    let mut pin = Input::new(pin, Pull::Up);

    loop {
        let msg = match read_sync_message(&mut pin).await {
            Ok(m) => m,
            Err(e) => {
                let _ = crate::SERIAL_CHANNEL.try_send(e);
                let _ = crate::SERIAL_CHANNEL.try_send("\r\n");
                continue;
            }
        };

        rx_channel.send(msg).await;
    }
}

async fn send_byte(pin: &mut Output<'_>, byte: u8) {
    let t0 = embassy_time::Instant::now();

    // Sync pulse with adaptive async timing
    pin.set_low();
    Timer::after_nanos(BIT_DELAY_NS).await;
    pin.set_high();
    let elapsed_ns = embassy_time::Instant::now().duration_since(t0).as_micros() * 1000;
    let target_ns = BIT_DELAY_NS * 2;
    if target_ns > elapsed_ns {
        Timer::after_nanos(target_ns - elapsed_ns).await;
    }

    // Send 8 data bits MSB first with adaptive async timing
    let mut parity = 0u8;
    for i in 0..8 {
        let bit = (byte >> (7 - i)) & 1;
        parity ^= bit;
        if bit == 1 {
            pin.set_high();
        } else {
            pin.set_low();
        }

        // Adaptive async wait for next bit
        let next_bit_target_ns = BIT_DELAY_NS * 2 + BIT_DELAY_NS * (i as u64 + 1);
        let elapsed_ns = embassy_time::Instant::now().duration_since(t0).as_micros() * 1000;
        if next_bit_target_ns > elapsed_ns {
            Timer::after_nanos(next_bit_target_ns - elapsed_ns).await;
        }
    }

    // Send even parity bit
    if parity == 1 {
        pin.set_high();
    } else {
        pin.set_low();
    }
    let next_bit_target_ns = BIT_DELAY_NS * 2 + BIT_DELAY_NS * 9;
    let elapsed_ns = embassy_time::Instant::now().duration_since(t0).as_micros() * 1000;
    if next_bit_target_ns > elapsed_ns {
        Timer::after_nanos(next_bit_target_ns - elapsed_ns).await;
    }

    // Return to idle high
    pin.set_high();
}

pub async fn secondary(
    pin: Peri<'static, PIN_1>,
    tx_channel: &'static Channel<ThreadModeRawMutex, SyncMessage, 8>,
) {
    let mut pin = Output::new(pin, Level::High);
    Timer::after_millis(1000).await;
    loop {
        let msg = tx_channel.receive().await;
        let (bytes, len) = msg.to_bytes();
        for &byte in bytes.iter().take(len) {
            send_byte(&mut pin, byte).await;
        }
    }
}
