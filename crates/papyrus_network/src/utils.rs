use std::collections::hash_map::{Keys, ValuesMut};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::stream::{Stream, StreamExt};

// This is an implementation of `StreamMap` from tokio_stream. The reason we're implementing it
// ourselves is that the implementation in tokio_stream requires that the values implement the
// Stream trait from tokio_stream and not from futures.
pub(crate) struct StreamHashMap<K: Unpin + Clone + Eq + Hash, V: Stream + Unpin> {
    map: HashMap<K, V>,
    finished_streams: HashSet<K>,
}

impl<K: Unpin + Clone + Eq + Hash, V: Stream + Unpin> StreamHashMap<K, V> {
    #[allow(dead_code)]
    pub fn new(map: HashMap<K, V>) -> Self {
        Self { map, finished_streams: Default::default() }
    }

    #[allow(dead_code)]
    pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
        self.map.values_mut()
    }

    #[allow(dead_code)]
    pub fn keys(&self) -> Keys<'_, K, V> {
        self.map.keys()
    }

    #[allow(dead_code)]
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.map.get_mut(key)
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.map.insert(key, value)
    }
}

impl<K: Unpin + Clone + Eq + Hash, V: Stream + Unpin> Stream for StreamHashMap<K, V> {
    type Item = (K, <V as Stream>::Item);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let unpinned_self = Pin::into_inner(self);
        let mut finished = true;
        for (key, stream) in &mut unpinned_self.map {
            match stream.poll_next_unpin(cx) {
                Poll::Ready(Some(value)) => {
                    return Poll::Ready(Some((key.clone(), value)));
                }
                Poll::Ready(None) => {
                    unpinned_self.finished_streams.insert(key.clone());
                }
                Poll::Pending => {
                    finished = false;
                }
            }
        }
        if finished {
            // TODO(shahak): Make StreamHashMap not end in order to accept new inserted streams.
            return Poll::Ready(None);
        }
        Poll::Pending
    }
}
