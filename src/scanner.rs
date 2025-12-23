use crate::debounce::Debounced;
use crate::keypin::{Keypin, KeypinEvent};
use core::task::Poll;
use futures_core::Stream;

pub enum ScanEvent {
    Down(u8),
    Up(u8),
}

pub struct Scanner<const N: usize> {
    pins: [Debounced<Keypin>; N],
}

impl<const N: usize> Scanner<N> {
    pub fn new(pins: [Keypin; N]) -> Self {
        Self {
            pins: pins.map(Debounced::new),
        }
    }
}

impl<const N: usize> Stream for Scanner<N> {
    type Item = ScanEvent;

    fn poll_next(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        for debounced_pin in this.pins.iter_mut() {
            let index = debounced_pin.inner.index;
            let mut pin = core::pin::Pin::new(debounced_pin);
            if let Poll::Ready(Some(event)) = pin.as_mut().poll_next(cx) {
                let scan_event = match event {
                    KeypinEvent::Down => ScanEvent::Down(index),
                    KeypinEvent::Up => ScanEvent::Up(index),
                };
                return Poll::Ready(Some(scan_event));
            }
        }

        Poll::Pending
    }
}
