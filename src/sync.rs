use embassy_rp::Peri;
use embassy_rp::clocks::clk_sys_freq;
use embassy_rp::gpio::{Level, Pull};
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::program::pio_asm;
use embassy_rp::pio::{Config, Direction, Pio, PioPin, ShiftDirection, StateMachine};

// Pretty stable at 5_000_000 so this gives us plenty of buffer.
const BIT_RATE: u64 = 500_000;
const TX_CYCLES_PER_BIT: u64 = 8;
const RX_CYCLES_PER_BIT: u64 = 8;

/// CRC-2 with polynomial x² + x + 1 (0b111).
/// Returns 2-bit CRC for 17-bit data.
fn crc2(data: u32) -> u32 {
    let mut crc = data & 0x1FFFF;
    for _ in 0..17 {
        if crc & (1 << 18) != 0 {
            crc ^= 0b111 << 16;
        }
        crc <<= 1;
    }
    (crc >> 17) & 0b11
}

/// Manchester-encode 19 bits (17 data + 2 CRC) into 38 half-bits (two words).
/// Encoding: data 0 → 01 (low-high), data 1 → 10 (high-low)
fn manchester_encode(data: u32) -> (u32, u32) {
    let mut lo: u32 = 0;
    for i in 0..16 {
        let bit = (data >> i) & 1;
        let manchester = if bit == 1 { 0b10 } else { 0b01 };
        lo |= manchester << (i * 2);
    }
    // Bits 16, 17, 18 go into hi (6 half-bits)
    let mut hi: u32 = 0;
    for i in 0..3 {
        let bit = (data >> (16 + i)) & 1;
        let manchester = if bit == 1 { 0b10 } else { 0b01 };
        hi |= manchester << (i * 2);
    }
    (lo, hi)
}

/// Manchester-decode 38 half-bits back to 19 bits.
/// Returns None if any bit pair is invalid (00 or 11).
fn manchester_decode(lo: u32, hi: u32) -> Option<u32> {
    let mut data: u32 = 0;
    for i in 0..16 {
        let pair = (lo >> (i * 2)) & 0b11;
        let bit = match pair {
            0b01 => 0,
            0b10 => 1,
            _ => return None,
        };
        data |= bit << i;
    }
    // Decode 3 bits from hi
    for i in 0..3 {
        let pair = (hi >> (i * 2)) & 0b11;
        let bit = match pair {
            0b01 => 0,
            0b10 => 1,
            _ => return None,
        };
        data |= bit << (16 + i);
    }
    Some(data)
}

pub struct SyncReceiver {
    sm: StateMachine<'static, PIO0, 0>,
}

impl SyncReceiver {
    pub fn new(pio: Peri<'static, PIO0>, pin: Peri<'static, impl PioPin>) -> Self {
        // Manchester receiver with idle detection for self-synchronization.
        // 1. Wait for line to be stable high (idle) for 16 cycles
        // 2. Wait for start pulse (line goes low, 16 cycles)
        // 3. Sample 32 Manchester half-bits (16 data bits), push
        // 4. Sample 6 more Manchester half-bits (3 bits: 1 data + 2 CRC), push
        let prg = pio_asm!(
            ".wrap_target",
            // Wait for idle: 16 consecutive high cycles
            "idle_wait:",
            "set x, 15",
            "idle_loop:",
            "jmp pin still_high", // if high, continue counting
            "jmp idle_wait",      // if low, restart
            "still_high:",
            "jmp x-- idle_loop",
            // Wait for start pulse (line goes low) with verification
            "wait_start:",
            "jmp pin wait_start",
            "set x, 3", // require 4 consecutive low samples to confirm START
            "start_verify:",
            "jmp pin wait_start", // if high, was a glitch, restart
            "jmp x-- start_verify",
            // After verification we're at cycle 9 of 16-cycle start pulse.
            // Wait until cycle 18 (middle of first half-bit): need 9 more cycles.
            "set y, 15 [7]", // 8 cycles (now at cycle 17)
            // Sample loop: each iteration samples 2 half-bits (one data bit)
            "bitloop1:",
            "in pins, 1",           // sample first half-bit (cycle 18, 26, ...)
            "nop [2]",              // delay 3 cycles
            "in pins, 1",           // sample second half-bit
            "jmp y-- bitloop1 [2]", // delay 3 cycles, loop (8 cycles per data bit)
            "push",                 // push 32 bits
            // Sample remaining 6 half-bits (3 bits: 1 data + 2 CRC)
            // Timing gap from sender is 2 cycles, push takes 1, so need 1 more
            "nop",
            "set y, 2", // 3 bits (6 half-bits)
            "bitloop2:",
            "in pins, 1",
            "nop [2]",
            "in pins, 1",
            "jmp y-- bitloop2 [2]",
            "push", // push 6 bits (padded to 32)
            ".wrap",
        );

        let Pio {
            mut common,
            mut sm0,
            ..
        } = Pio::new(pio, crate::Irqs);

        let mut sync_pin = common.make_pio_pin(pin);
        sync_pin.set_pull(Pull::Up);

        let mut cfg = Config::default();
        cfg.use_program(&common.load_program(&prg.program), &[]);
        cfg.set_in_pins(&[&sync_pin]);
        cfg.set_jmp_pin(&sync_pin);
        cfg.shift_in.direction = ShiftDirection::Right;

        let pio_freq = BIT_RATE * RX_CYCLES_PER_BIT;
        let divider_256 = (clk_sys_freq() as u64) * 256 / pio_freq;
        cfg.clock_divider =
            fixed::FixedU32::<fixed::types::extra::U8>::from_bits(divider_256 as u32);

        sm0.set_config(&cfg);
        sm0.set_pin_dirs(Direction::In, &[&sync_pin]);
        sm0.set_enable(true);

        Self { sm: sm0 }
    }

    /// Receive and decode a Manchester-encoded frame with CRC verification.
    /// Returns None if encoding is invalid or CRC check fails.
    pub async fn recv(&mut self) -> Option<u32> {
        let lo = self.sm.rx().wait_pull().await;
        let hi = self.sm.rx().wait_pull().await;
        // hi has 6 bits shifted in; with shift-right they're at bits 26-31
        let raw = manchester_decode(lo, hi >> 26)?;
        // raw contains 17 data bits + 2 CRC bits
        let data = raw & 0x1FFFF;
        let received_crc = (raw >> 17) & 0b11;
        if crc2(data) != received_crc {
            return None;
        }
        Some(data)
    }
}

pub struct SyncSender {
    sm: StateMachine<'static, PIO0, 0>,
}

impl SyncSender {
    pub fn new(pio: Peri<'static, PIO0>, pin: Peri<'static, impl PioPin>) -> Self {
        // Manchester sender:
        // 1. Pull first word (32 Manchester half-bits = 16 data bits)
        // 2. Send START pulse (16 cycles low)
        // 3. Send 32 half-bits at 4 cycles each
        // 4. Pull second word (6 Manchester half-bits = 3 bits: 1 data + 2 CRC)
        // 5. Send 6 half-bits
        // 6. Return to idle (high)
        let prg = pio_asm!(
            ".wrap_target",
            "pull block",       // wait for first 32 Manchester bits
            "set pins, 0 [15]", // START: low for 16 cycles
            "set x, 31",        // 32 half-bits to send
            "loop1:",
            "out pins, 1 [2]", // output bit, hold 3 cycles
            "jmp x-- loop1",   // 1 cycle, total 4 per half-bit
            "pull noblock",    // get remaining 6 Manchester bits (don't block)
            "set x, 5",        // 6 half-bits to send (3 bits: 1 data + 2 CRC)
            "loop2:",
            "out pins, 1 [2]", // output bit
            "jmp x-- loop2",
            "set pins, 1 [31]", // return to idle
            "nop [31]",         // hold idle for 64 cycles total (receiver needs 32)
            ".wrap",
        );

        let Pio {
            mut common,
            mut sm0,
            ..
        } = Pio::new(pio, crate::Irqs);

        let sync_pin = common.make_pio_pin(pin);

        let mut cfg = Config::default();
        cfg.use_program(&common.load_program(&prg.program), &[]);
        cfg.set_out_pins(&[&sync_pin]);
        cfg.set_set_pins(&[&sync_pin]);
        cfg.shift_out.direction = ShiftDirection::Right;

        let pio_freq = BIT_RATE * TX_CYCLES_PER_BIT;
        let divider_256 = (clk_sys_freq() as u64) * 256 / pio_freq;
        cfg.clock_divider =
            fixed::FixedU32::<fixed::types::extra::U8>::from_bits(divider_256 as u32);

        sm0.set_config(&cfg);
        sm0.set_pin_dirs(Direction::Out, &[&sync_pin]);
        sm0.set_pins(Level::High, &[&sync_pin]);
        sm0.set_enable(true);

        Self { sm: sm0 }
    }

    pub async fn send(&mut self, data: u32) {
        let data = data & 0x1FFFF;
        let crc = crc2(data);
        let frame = data | (crc << 17); // 17 data bits + 2 CRC bits
        let (lo, hi) = manchester_encode(frame);
        // Push both words; PIO will pull them in sequence
        self.sm.tx().wait_push(lo).await;
        self.sm.tx().wait_push(hi).await;
    }
}
