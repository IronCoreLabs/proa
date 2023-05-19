use std::time::Duration;

use futures::Stream;

use super::holistic_timeout::HolisticTimeout;

pub trait HolisticStreamExt: Stream {
    /// Applies a timeout to the entire passed stream.
    /// A clone of [`tokio_stream::StreamExt::timeout`](https://docs.rs/tokio-stream/latest/tokio_stream/trait.StreamExt.html#method.timeout) that applies to the entire stream instead of per-item.
    fn holistic_timeout(self, duration: Duration) -> HolisticTimeout<Self>
    where
        Self: Sized,
    {
        HolisticTimeout::new(self, duration)
    }
}

impl<St: ?Sized> HolisticStreamExt for St where St: Stream {}
