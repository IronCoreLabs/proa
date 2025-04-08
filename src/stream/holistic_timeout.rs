use futures::stream::Fuse;
use futures::{Future, Stream, StreamExt, ready};
use pin_project::pin_project;
use tokio::time::{Instant, Sleep};

use core::pin::Pin;
use core::task::{Context, Poll};
use std::fmt;
use std::time::Duration;

/// Stream returned by the [`timeout`](super::HolisticStreamExt::holistic_timeout) method.
/// Mostly a clone of `timeout` from tokio, we just don't reset the duration of the timeout on
/// each iteration.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
#[pin_project]
pub struct HolisticTimeout<S> {
    #[pin]
    stream: Fuse<S>,
    #[pin]
    deadline: Sleep,
    duration: Duration,
    poll_deadline: bool,
}

/// Error returned by `Timeout` and `TimeoutRepeating`.
#[derive(Debug, PartialEq, Eq)]
pub struct Elapsed(());

impl<S: Stream> HolisticTimeout<S> {
    pub(super) fn new(stream: S, duration: Duration) -> Self {
        let timeout = Instant::now() + duration;
        let deadline = tokio::time::sleep_until(timeout);

        HolisticTimeout {
            stream: stream.fuse(),
            deadline,
            duration,
            poll_deadline: true,
        }
    }
}

impl<S: Stream> Stream for HolisticTimeout<S> {
    type Item = Result<S::Item, Elapsed>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let me = self.project();

        match me.stream.poll_next(cx) {
            Poll::Ready(v) => {
                if v.is_some() {
                    *me.poll_deadline = true;
                }
                return Poll::Ready(v.map(Ok));
            }
            Poll::Pending => {}
        };

        if *me.poll_deadline {
            ready!(me.deadline.poll(cx));
            *me.poll_deadline = false;
            return Poll::Ready(Some(Err(Elapsed::new())));
        }

        Poll::Pending
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, upper) = self.stream.size_hint();

        // The timeout stream may insert an error before and after each message
        // from the underlying stream, but no more than one error between each
        // message. Hence the upper bound is computed as 2x+1.

        // Using a helper function to enable use of question mark operator.
        fn twice_plus_one(value: Option<usize>) -> Option<usize> {
            value?.checked_mul(2)?.checked_add(1)
        }

        (lower, twice_plus_one(upper))
    }
}

impl Elapsed {
    pub(crate) fn new() -> Self {
        Elapsed(())
    }
}

impl fmt::Display for Elapsed {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        "deadline has elapsed".fmt(fmt)
    }
}

impl std::error::Error for Elapsed {}

impl From<Elapsed> for std::io::Error {
    fn from(_err: Elapsed) -> std::io::Error {
        std::io::ErrorKind::TimedOut.into()
    }
}
