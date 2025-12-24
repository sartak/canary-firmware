use usbd_hid::descriptor::KeyboardReport;

use crate::config::{LEFT_KEYS, NUM_KEYS, RIGHT_KEYS};
use crate::stash::Hand;

#[derive(Default, Clone, Copy)]
pub struct Matrix(u64);

impl Matrix {
    fn bit_index(hand: Hand, index: u8) -> u8 {
        match hand {
            Hand::Left => index,
            Hand::Right => index + 32,
        }
    }

    pub fn set_down(&mut self, hand: Hand, index: u8) {
        self.0 |= 1 << Self::bit_index(hand, index);
    }

    pub fn set_up(&mut self, hand: Hand, index: u8) {
        self.0 &= !(1 << Self::bit_index(hand, index));
    }

    pub fn is_down(&self, hand: Hand, index: u8) -> bool {
        (self.0 & (1 << Self::bit_index(hand, index))) != 0
    }

    pub fn keyboard_report(self) -> KeyboardReport {
        let mut report = KeyboardReport::default();
        let mut slot = 0;

        for (keys, hand) in [(&LEFT_KEYS, Hand::Left), (&RIGHT_KEYS, Hand::Right)] {
            for (i, key) in keys.iter().enumerate().take(NUM_KEYS) {
                if self.is_down(hand, i as u8)
                    && let Some(hid_code) = key.tap.to_hid_code()
                    && slot < 6
                {
                    report.keycodes[slot] = hid_code;
                    slot += 1;
                }
            }
        }

        report
    }
}
