use crate::debounce::Debounced;
use crate::keypin::{Keypin, KeypinEvent};
use core::task::Poll;
use futures_core::Stream;

pub enum MatrixEvent {
    KeyDown(&'static str, Option<char>),
    KeyUp(&'static str, Option<char>),
}

pub struct Matrix<const N: usize> {
    pins: [Debounced<Keypin>; N],
}

impl<const N: usize> Matrix<N> {
    pub fn new(pins: [Keypin; N]) -> Self {
        Self {
            pins: pins.map(Debounced::new),
        }
    }
}

impl<const N: usize> Stream for Matrix<N> {
    type Item = MatrixEvent;

    fn poll_next(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        for debounced_pin in this.pins.iter_mut() {
            let pin_label = debounced_pin.inner.label;
            let pin_keycode = debounced_pin.inner.keycode;

            let mut pin = core::pin::Pin::new(debounced_pin);
            if let Poll::Ready(Some(event)) = pin.as_mut().poll_next(cx) {
                let matrix_event = match event {
                    KeypinEvent::Down => MatrixEvent::KeyDown(pin_label, pin_keycode),
                    KeypinEvent::Up => MatrixEvent::KeyUp(pin_label, pin_keycode),
                };
                return Poll::Ready(Some(matrix_event));
            }
        }

        Poll::Pending
    }
}
