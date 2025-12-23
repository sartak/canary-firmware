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
}
