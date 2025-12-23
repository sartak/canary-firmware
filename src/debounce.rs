use crate::keypin::KeypinEvent;
use core::task::Poll;
use embassy_time::{Duration, Instant};
use futures_core::Stream;

// Kailh Choc v1 (PG1350) specifies ≤10ms bounce time
const DEBOUNCE_MS: u64 = 10;

pub struct Debounced<S> {
    pub inner: S,
    pending: Option<(KeypinEvent, Instant)>,
    emitted_down: bool,
}

impl<S> Debounced<S>
where
    S: Stream<Item = KeypinEvent>,
{
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            pending: None,
            emitted_down: false,
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
        // Poll inner for new events
        let inner = core::pin::Pin::new(&mut self.inner);
        if let Poll::Ready(Some(event)) = inner.poll_next(cx) {
            // New event resets the debounce timer
            self.pending = Some((event, Instant::now()));
        }

        // Check if pending event has been stable long enough
        if let Some((event, since)) = self.pending {
            let elapsed = Instant::now().duration_since(since);
            if elapsed >= Duration::from_millis(DEBOUNCE_MS) {
                // Only emit if it's a state change from what we last emitted
                let dominated = match event {
                    KeypinEvent::Down => self.emitted_down,
                    KeypinEvent::Up => !self.emitted_down,
                };
                self.pending = None;
                if !dominated {
                    match event {
                        KeypinEvent::Down => self.emitted_down = true,
                        KeypinEvent::Up => self.emitted_down = false,
                    }
                    return Poll::Ready(Some(event));
                }
            } else {
                // Schedule wake when debounce period expires
                cx.waker().wake_by_ref();
            }
        }

        Poll::Pending
    }
}
