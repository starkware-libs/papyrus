use super::traits::*;
use anyhow::bail;
use arrayvec::ArrayVec;
use derive_more::*;
use std::fmt::Display;

#[derive(
    Clone,
    Copy,
    Debug,
    Deref,
    DerefMut,
    Default,
    Display,
    PartialEq,
    Eq,
    From,
    PartialOrd,
    Ord,
    Hash,
)]
pub struct CutStart<T>(pub T);

impl Encodable for () {
    type Encoded = [u8; 0];

    fn encode(self) -> Self::Encoded {
        []
    }
}

impl Decodable for () {
    fn decode(b: &[u8]) -> anyhow::Result<Self> {
        if !b.is_empty() {
            return Err(TooLong::<0> { received: b.len() }.into());
        }

        Ok(())
    }
}

impl Encodable for Vec<u8> {
    type Encoded = Self;

    fn encode(self) -> Self::Encoded {
        self
    }
}

impl Decodable for Vec<u8> {
    fn decode(b: &[u8]) -> anyhow::Result<Self> {
        Ok(b.to_vec())
    }
}

#[cfg(feature = "bytes")]
impl Encodable for bytes::Bytes {
    type Encoded = Self;

    fn encode(self) -> Self::Encoded {
        self
    }
}

#[cfg(feature = "bytes")]
impl Decodable for bytes::Bytes {
    fn decode(b: &[u8]) -> anyhow::Result<Self> {
        Ok(b.to_vec().into())
    }
}

impl Encodable for String {
    type Encoded = Vec<u8>;

    fn encode(self) -> Self::Encoded {
        self.into_bytes()
    }
}

impl Decodable for String {
    fn decode(b: &[u8]) -> anyhow::Result<Self> {
        Ok(String::from_utf8(b.into())?)
    }
}

impl<const MAX_LEN: usize> Encodable for ArrayVec<u8, MAX_LEN> {
    type Encoded = Self;

    fn encode(self) -> Self::Encoded {
        self
    }
}

impl<const MAX_LEN: usize> Decodable for ArrayVec<u8, MAX_LEN> {
    fn decode(v: &[u8]) -> anyhow::Result<Self> {
        let mut out = Self::default();
        out.try_extend_from_slice(v)?;
        Ok(out)
    }
}

impl<const LEN: usize> Encodable for [u8; LEN] {
    type Encoded = Self;

    fn encode(self) -> Self::Encoded {
        self
    }
}

impl<const LEN: usize> Decodable for [u8; LEN] {
    fn decode(b: &[u8]) -> anyhow::Result<Self> {
        if b.len() != LEN {
            return Err(BadLength::<LEN> { received: b.len() }.into());
        }

        let mut l = [0; LEN];
        l.copy_from_slice(b);
        Ok(l)
    }
}

#[derive(Clone, Debug)]
pub struct BadLength<const EXPECTED: usize> {
    pub received: usize,
}

impl<const EXPECTED: usize> Display for BadLength<EXPECTED> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bad length: {EXPECTED} != {}", self.received)
    }
}

impl<const EXPECTED: usize> std::error::Error for BadLength<EXPECTED> {}

#[derive(Clone, Debug)]
pub struct TooShort<const MINIMUM: usize> {
    pub received: usize,
}

impl<const MINIMUM: usize> Display for TooShort<MINIMUM> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Value too short: {} < {MINIMUM}", self.received)
    }
}

impl<const MINIMUM: usize> std::error::Error for TooShort<MINIMUM> {}

#[derive(Clone, Debug)]
pub struct TooLong<const MAXIMUM: usize> {
    pub received: usize,
}
impl<const MAXIMUM: usize> Display for TooLong<MAXIMUM> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Value too long: {} > {MAXIMUM}", self.received)
    }
}

impl<const MAXIMUM: usize> std::error::Error for TooLong<MAXIMUM> {}

#[macro_export]
macro_rules! table_integer {
    ($ty:ident => $real_ty:ident) => {
        impl $crate::orm::Encodable for $ty {
            type Encoded = [u8; $real_ty::BITS as usize / 8];

            fn encode(self) -> Self::Encoded {
                self.to_be_bytes()
            }
        }

        impl $crate::orm::Decodable for $ty {
            fn decode(b: &[u8]) -> anyhow::Result<Self> {
                const EXPECTED: usize = $real_ty::BITS as usize / 8;

                match b.len() {
                    EXPECTED => Ok($real_ty::from_be_bytes(*$crate::arrayref::array_ref!(
                        &*b, 0, EXPECTED
                    ))
                    .into()),
                    other => Err($crate::orm::BadLength::<EXPECTED> { received: other }.into()),
                }
            }
        }
    };
}

table_integer!(u32 => u32);
table_integer!(u64 => u64);
table_integer!(u128 => u128);

impl<T, const LEN: usize> Encodable for CutStart<T>
where
    T: Encodable<Encoded = [u8; LEN]>,
{
    type Encoded = ArrayVec<u8, LEN>;

    fn encode(self) -> Self::Encoded {
        let arr = self.0.encode();

        let mut out = <Self::Encoded as Default>::default();
        out.try_extend_from_slice(&arr[arr.iter().take_while(|b| **b == 0).count()..])
            .unwrap();
        out
    }
}

impl<T, const LEN: usize> Decodable for CutStart<T>
where
    T: Encodable<Encoded = [u8; LEN]> + Decodable,
{
    fn decode(b: &[u8]) -> anyhow::Result<Self> {
        if b.len() > LEN {
            return Err(TooLong::<LEN> { received: b.len() }.into());
        }

        let mut array = [0; LEN];
        array[LEN - b.len()..].copy_from_slice(b);
        T::decode(&array).map(Self)
    }
}

#[cfg(feature = "cbor")]
#[macro_export]
macro_rules! cbor_table_object {
    ($ty:ident) => {
        impl Encodable for $ty {
            type Encoded = Vec<u8>;

            fn encode(self) -> Self::Encoded {
                let mut v = vec![];
                $crate::ciborium::ser::into_writer(&self, &mut v).unwrap();
                v
            }
        }

        impl Decodable for $ty {
            fn decode(v: &[u8]) -> anyhow::Result<Self> {
                Ok($crate::ciborium::de::from_reader(v)?)
            }
        }
    };
}

impl<A, B, const A_LEN: usize, const B_LEN: usize> Encodable for (A, B)
where
    A: TableObject<Encoded = [u8; A_LEN]>,
    B: TableObject<Encoded = [u8; B_LEN]>,
{
    type Encoded = Vec<u8>;

    fn encode(self) -> Self::Encoded {
        let mut v = Vec::with_capacity(A_LEN + B_LEN);
        v.extend_from_slice(&self.0.encode());
        v.extend_from_slice(&self.1.encode());
        v
    }
}

impl<A, B, const A_LEN: usize, const B_LEN: usize> Decodable for (A, B)
where
    A: TableObject<Encoded = [u8; A_LEN]>,
    B: TableObject<Encoded = [u8; B_LEN]>,
{
    fn decode(v: &[u8]) -> anyhow::Result<Self> {
        if v.len() != A_LEN + B_LEN {
            bail!("Bad length: {} != {} + {}", v.len(), A_LEN, B_LEN);
        }
        Ok((
            A::decode(&v[..A_LEN]).unwrap(),
            B::decode(&v[A_LEN..]).unwrap(),
        ))
    }
}
