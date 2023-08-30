pub mod handler;
pub mod protocol;

use derive_more::Display;

#[derive(Clone, Copy, Debug, Default, Display, Eq, Hash, PartialEq)]
pub struct RequestId(pub usize);
