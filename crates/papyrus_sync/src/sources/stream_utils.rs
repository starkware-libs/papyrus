use async_trait::async_trait;
use futures::channel::mpsc::channel;
use futures::{SinkExt, StreamExt};
use futures_channel::mpsc::Receiver;

// An extension trait for `StreamExt`s that provides methods for splitting and consuming chunks from
// streams.
#[async_trait]
pub trait MyStreamExt: StreamExt {
    // Fanout items to multiple sinks.
    fn fanout(self, buffer: usize) -> (Receiver<Self::Item>, Receiver<Self::Item>)
    where
        Self: Sized + Send + 'static,
        Self::Item: Clone + Send + 'static;

    // Consume exactly n items.
    async fn take_n(&mut self, n: usize) -> Option<Vec<Self::Item>>
    where
        Self: Sized + Unpin + Send + 'static,
        Self::Item: Clone + Send + 'static;
}
#[async_trait]
impl<T: StreamExt> MyStreamExt for T {
    fn fanout(self, buffer: usize) -> (Receiver<Self::Item>, Receiver<Self::Item>)
    where
        Self: Sized + Send + 'static,
        Self::Item: Clone + Send + 'static,
    {
        let (tx0, rx0) = channel(buffer);
        let (tx1, rx1) = channel(buffer);
        let joint_sink = tx0.fanout(tx1);

        let forward_future = self.map(Ok).forward(joint_sink);
        tokio::spawn(forward_future);

        (rx0, rx1)
    }

    async fn take_n(&mut self, n: usize) -> Option<Vec<Self::Item>>
    where
        Self: Sized + Unpin + Send + 'static,
        Self::Item: Clone + Send + 'static,
    {
        if n == 0 {
            return Some(vec![]);
        }
        let mut res = Vec::new();
        res.reserve(n);
        while let Some(item) = self.next().await {
            res.push(item);
            if res.len() == n {
                return Some(res);
            }
        }
        None
    }
}
