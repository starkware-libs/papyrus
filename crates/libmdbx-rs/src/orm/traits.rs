use std::fmt::Debug;

pub trait Encodable: Send + Sync + Sized {
    type Encoded: AsRef<[u8]> + Send + Sync;

    fn encode(self) -> Self::Encoded;
}

pub trait Decodable: Send + Sync + Sized {
    fn decode(b: &[u8]) -> anyhow::Result<Self>;
}

pub trait TableObject: Encodable + Decodable {}

impl<T> TableObject for T where T: Encodable + Decodable {}

pub trait Table: Send + Sync + Debug + 'static {
    const NAME: &'static str;

    type Key: Encodable;
    type Value: TableObject;
    type SeekKey: Encodable;
}
pub trait DupSort: Table {
    type SeekValue: Encodable;
}
