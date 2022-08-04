use std::time::Duration;

use async_stream::stream;
use async_trait::async_trait;
use futures::channel::mpsc::channel;
use futures::stream::{self};
use futures::{SinkExt, Stream, StreamExt};
use futures_channel::mpsc::Receiver;
use tokio::time::sleep;

#[async_trait]
pub trait MyStreamExt: StreamExt {
    fn fanout(self, buffer: usize) -> (Receiver<Self::Item>, Receiver<Self::Item>)
    where
        Self: Sized + Send + 'static,
        Self::Item: Clone + Send + 'static;

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

async fn my_stream(
    input: impl Stream<Item = usize> + Send + 'static,
) -> impl Stream<Item = (usize, Vec<usize>)> {
    let (ns0, mut ns1) = input
        .map(|n| async move {
            // Fetch n from web.
            println!("A+");
            sleep(Duration::from_secs(2)).await;
            println!("A-");
            n
        })
        .buffered(5)
        .fanout(10);
    let mut flat_classes = ns0
        .flat_map(|n| stream::iter(vec![1]).cycle().take(n))
        .map(|_x| async move {
            println!("C+");
            sleep(Duration::from_secs(1)).await;
            println!("C-");
            // Download from web.
            2
        })
        .buffered(5);
    let s = stream! {
        while let Some(n) = ns1.next().await{
            println!("D");
            let res = flat_classes.take_n(n).await.unwrap();
            yield (n, res);
        }
    };
    s
}

// #[tokio::main]
// async fn main() {
//     let v: Vec<_> = my_stream(stream::iter(vec![10, 3, 0, 1, 10, 7, 3,
// 8])).await.collect().await;     println!("{:?}", v);
// }
