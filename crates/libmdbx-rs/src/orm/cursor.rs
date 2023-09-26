use super::traits::*;
use crate::{TransactionKind, WriteFlags, RW};
use std::marker::PhantomData;

#[derive(Clone, Debug)]
pub(crate) struct DecodableWrapper<T>(pub T);

impl<'tx, T> crate::Decodable<'tx> for DecodableWrapper<T>
where
    T: Decodable,
{
    fn decode(data_val: &[u8]) -> Result<Self, crate::Error>
    where
        Self: Sized,
    {
        T::decode(data_val)
            .map_err(|e| crate::Error::DecodeError(e.into()))
            .map(Self)
    }
}

#[derive(Debug)]
pub struct Cursor<'tx, K, T>
where
    K: TransactionKind,
    T: Table,
{
    pub(crate) inner: crate::Cursor<'tx, K>,
    pub(crate) _marker: PhantomData<T>,
}

#[allow(clippy::type_complexity)]
fn map_res_inner<T, E>(
    v: Result<Option<(DecodableWrapper<T::Key>, DecodableWrapper<T::Value>)>, E>,
) -> anyhow::Result<Option<(T::Key, T::Value)>>
where
    T: Table,
    <T as Table>::Key: Decodable,
    E: std::error::Error + Send + Sync + 'static,
{
    if let Some((k, v)) = v? {
        return Ok(Some((k.0, v.0)));
    }

    Ok(None)
}

impl<'tx, K, T> Cursor<'tx, K, T>
where
    K: TransactionKind,
    T: Table,
{
    pub fn first(&mut self) -> anyhow::Result<Option<(T::Key, T::Value)>>
    where
        T::Key: Decodable,
    {
        map_res_inner::<T, _>(self.inner.first())
    }

    pub fn seek_closest(&mut self, key: T::SeekKey) -> anyhow::Result<Option<(T::Key, T::Value)>>
    where
        T::Key: Decodable,
    {
        map_res_inner::<T, _>(self.inner.set_range(key.encode().as_ref()))
    }

    pub fn seek_exact(&mut self, key: T::Key) -> anyhow::Result<Option<(T::Key, T::Value)>>
    where
        T::Key: Decodable,
    {
        map_res_inner::<T, _>(self.inner.set_key(key.encode().as_ref()))
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> anyhow::Result<Option<(T::Key, T::Value)>>
    where
        T::Key: Decodable,
    {
        map_res_inner::<T, _>(self.inner.next())
    }

    pub fn prev(&mut self) -> anyhow::Result<Option<(T::Key, T::Value)>>
    where
        T::Key: Decodable,
    {
        map_res_inner::<T, _>(self.inner.prev())
    }

    pub fn last(&mut self) -> anyhow::Result<Option<(T::Key, T::Value)>>
    where
        T::Key: Decodable,
    {
        map_res_inner::<T, _>(self.inner.last())
    }

    pub fn current(&mut self) -> anyhow::Result<Option<(T::Key, T::Value)>>
    where
        T::Key: Decodable,
    {
        map_res_inner::<T, _>(self.inner.get_current())
    }

    pub fn walk(
        self,
        start: Option<T::SeekKey>,
    ) -> impl Iterator<Item = anyhow::Result<(T::Key, T::Value)>>
    where
        T: Table,
        T::Key: Decodable,
    {
        struct I<'tx, K, T>
        where
            K: TransactionKind,
            T: Table,
            T::Key: Decodable,
        {
            cursor: Cursor<'tx, K, T>,
            start: Option<T::SeekKey>,

            first: bool,
        }

        impl<'tx, K, T> Iterator for I<'tx, K, T>
        where
            K: TransactionKind,
            T: Table,
            T::Key: Decodable,
        {
            type Item = anyhow::Result<(T::Key, T::Value)>;

            fn next(&mut self) -> Option<Self::Item> {
                if self.first {
                    self.first = false;
                    if let Some(start) = self.start.take() {
                        self.cursor.seek_closest(start)
                    } else {
                        self.cursor.first()
                    }
                } else {
                    self.cursor.next()
                }
                .transpose()
            }
        }

        I {
            cursor: self,
            start,
            first: true,
        }
    }

    pub fn walk_back(
        self,
        start: Option<T::SeekKey>,
    ) -> impl Iterator<Item = anyhow::Result<(T::Key, T::Value)>>
    where
        T: Table,
        T::Key: Decodable,
    {
        struct I<'tx, K, T>
        where
            K: TransactionKind,
            T: Table,
            T::Key: Decodable,
        {
            cursor: Cursor<'tx, K, T>,
            start: Option<T::SeekKey>,

            first: bool,
        }

        impl<'tx, K, T> Iterator for I<'tx, K, T>
        where
            K: TransactionKind,
            T: Table,
            T::Key: Decodable,
        {
            type Item = anyhow::Result<(T::Key, T::Value)>;

            fn next(&mut self) -> Option<Self::Item> {
                if self.first {
                    self.first = false;
                    if let Some(start_key) = self.start.take() {
                        self.cursor.seek_closest(start_key)
                    } else {
                        self.cursor.last()
                    }
                } else {
                    self.cursor.prev()
                }
                .transpose()
            }
        }

        I {
            cursor: self,
            start,
            first: true,
        }
    }
}

impl<'tx, K, T> Cursor<'tx, K, T>
where
    K: TransactionKind,
    T: DupSort,
{
    pub fn seek_value(
        &mut self,
        key: T::Key,
        seek_value: T::SeekValue,
    ) -> anyhow::Result<Option<T::Value>>
    where
        T::Key: Clone,
    {
        let res = self.inner.get_both_range::<DecodableWrapper<T::Value>>(
            key.encode().as_ref(),
            seek_value.encode().as_ref(),
        )?;

        if let Some(v) = res {
            return Ok(Some(v.0));
        }

        Ok(None)
    }

    pub fn last_value(&mut self) -> anyhow::Result<Option<T::Value>>
    where
        T::Key: Decodable,
    {
        Ok(self
            .inner
            .last_dup::<DecodableWrapper<T::Value>>()?
            .map(|v| v.0))
    }

    pub fn next_key(&mut self) -> anyhow::Result<Option<(T::Key, T::Value)>>
    where
        T::Key: Decodable,
    {
        map_res_inner::<T, _>(self.inner.next_nodup())
    }

    pub fn next_value(&mut self) -> anyhow::Result<Option<(T::Key, T::Value)>>
    where
        T::Key: Decodable,
    {
        map_res_inner::<T, _>(self.inner.next_dup())
    }

    pub fn prev_key(&mut self) -> anyhow::Result<Option<(T::Key, T::Value)>>
    where
        T::Key: Decodable,
    {
        map_res_inner::<T, _>(self.inner.prev_nodup())
    }

    pub fn prev_value(&mut self) -> anyhow::Result<Option<(T::Key, T::Value)>>
    where
        T::Key: Decodable,
    {
        map_res_inner::<T, _>(self.inner.prev_dup())
    }

    pub fn walk_key(
        self,
        start: T::Key,
        seek_value: Option<T::SeekValue>,
    ) -> impl Iterator<Item = anyhow::Result<T::Value>>
    where
        T::Key: Clone + Decodable,
    {
        struct I<'tx, K, T>
        where
            K: TransactionKind,
            T: DupSort,
            T::Key: Clone + Decodable,
        {
            cursor: Cursor<'tx, K, T>,
            start: Option<T::Key>,
            seek_value: Option<T::SeekValue>,

            first: bool,
        }

        impl<'tx, K, T> Iterator for I<'tx, K, T>
        where
            K: TransactionKind,
            T: DupSort,
            T::Key: Clone + Decodable,
        {
            type Item = anyhow::Result<T::Value>;

            fn next(&mut self) -> Option<Self::Item> {
                if self.first {
                    self.first = false;
                    let start_key = self.start.take().unwrap();
                    if let Some(seek_both_key) = self.seek_value.take() {
                        self.cursor.seek_value(start_key, seek_both_key)
                    } else {
                        self.cursor.seek_exact(start_key).map(|v| v.map(|(_, v)| v))
                    }
                } else {
                    self.cursor.next_value().map(|v| v.map(|(_, v)| v))
                }
                .transpose()
            }
        }

        I {
            cursor: self,
            start: Some(start),
            seek_value,
            first: true,
        }
    }
}

impl<'tx, T> Cursor<'tx, RW, T>
where
    T: Table,
{
    pub fn upsert(&mut self, key: T::Key, value: T::Value) -> anyhow::Result<()> {
        Ok(self.inner.put(
            key.encode().as_ref(),
            value.encode().as_ref(),
            WriteFlags::UPSERT,
        )?)
    }

    pub fn append(&mut self, key: T::Key, value: T::Value) -> anyhow::Result<()> {
        Ok(self.inner.put(
            key.encode().as_ref(),
            value.encode().as_ref(),
            WriteFlags::APPEND,
        )?)
    }

    pub fn delete_current(&mut self) -> anyhow::Result<()> {
        self.inner.del(WriteFlags::CURRENT)?;

        Ok(())
    }
}

impl<'tx, T> Cursor<'tx, RW, T>
where
    T: DupSort,
{
    pub fn delete_current_key(&mut self) -> anyhow::Result<()> {
        Ok(self.inner.del(WriteFlags::NO_DUP_DATA)?)
    }
    pub fn append_value(&mut self, key: T::Key, value: T::Value) -> anyhow::Result<()> {
        Ok(self.inner.put(
            key.encode().as_ref(),
            value.encode().as_ref(),
            WriteFlags::APPEND_DUP,
        )?)
    }
}
