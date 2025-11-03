use core::task::Poll;
use embassy_rp::gpio::{Input, Pull};
use futures_core::Stream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeypinEvent {
    Down,
    Up,
}

pub struct Keypin {
    gpio: Input<'static>,
    pub label: &'static str,
    is_down: bool,
}

impl Keypin {
    pub fn new(
        pin: embassy_rp::Peri<'static, impl embassy_rp::gpio::Pin>,
        label: &'static str,
    ) -> Self {
        Self {
            gpio: Input::new(pin, Pull::Up),
            label,
            is_down: false,
        }
    }
}

impl Stream for Keypin {
    type Item = KeypinEvent;

    fn poll_next(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.is_down {
            let fut = this.gpio.wait_for_high();
            futures_util::pin_mut!(fut);
            match fut.as_mut().poll(cx) {
                Poll::Ready(()) => {
                    this.is_down = false;
                    Poll::Ready(Some(KeypinEvent::Up))
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            let fut = this.gpio.wait_for_low();
            futures_util::pin_mut!(fut);
            match fut.as_mut().poll(cx) {
                Poll::Ready(()) => {
                    this.is_down = true;
                    Poll::Ready(Some(KeypinEvent::Down))
                }
                Poll::Pending => Poll::Pending,
            }
        }
    }
}
