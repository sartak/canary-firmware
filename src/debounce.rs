use crate::keypin::KeypinEvent;
use core::task::Poll;
use embassy_time::{Duration, Instant};
use futures_core::Stream;

const DEBOUNCE_MS: u64 = 15;

pub struct Debounced<S> {
    pub inner: S,
    last_event_time: Option<Instant>,
}

impl<S> Debounced<S>
where
    S: Stream<Item = KeypinEvent>,
{
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            last_event_time: None,
        }
    }
}

impl<S> Stream for Debounced<S>
where
    S: Stream<Item = KeypinEvent> + Unpin,
{
    type Item = KeypinEvent;

    fn poll_next(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let inner = core::pin::Pin::new(&mut self.inner);
        match inner.poll_next(cx) {
            Poll::Ready(Some(event)) => {
                let now = Instant::now();
                let should_emit = self
                    .last_event_time
                    .map(|last| now.duration_since(last) >= Duration::from_millis(DEBOUNCE_MS))
                    .unwrap_or(true);

                if should_emit {
                    self.last_event_time = Some(now);
                    Poll::Ready(Some(event))
                } else {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
