// This is an implementation of `StreamMap` from tokio_stream. The reason we're implementing it
// ourselves is that the implementation in tokio_stream requires that the values implement the
// Stream trait from tokio_stream and not from futures.
pub(crate) struct StreamHashMap<K: Unpin + Clone + Eq + Hash, V: StreamTrait + Unpin> {
    map: HashMap<K, V>,
    finished_streams: HashSet<K>,
}

impl<K: Unpin + Clone + Eq + Hash, V: StreamTrait + Unpin> StreamHashMap<K, V> {
    pub fn new(map: HashMap<K, V>) -> Self {
        Self { map, finished_streams: Default::default() }
    }

    pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
        self.map.values_mut()
    }

    pub fn keys(&self) -> Keys<'_, K, V> {
        self.map.keys()
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.map.get_mut(key)
    }
}

impl<K: Unpin + Clone + Eq + Hash, V: StreamTrait + Unpin> StreamTrait for StreamHashMap<K, V> {
    type Item = (K, <V as StreamTrait>::Item);

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
            return Poll::Ready(None);
        }
        Poll::Pending
    }
}
