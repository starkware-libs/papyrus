//! Fully typed ORM based on libmdbx.
//!
//! Much simpler in usage but slightly more limited.
//!
//! ```rust,no_run
//! use libmdbx::orm::{table, table_info, DatabaseChart, Decodable, Encodable};
//! use std::sync::Arc;
//! use once_cell::sync::Lazy;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
//! pub struct UserInfo {
//!     pub age: u8,
//!     pub first_name: String,
//!     pub last_name: String,
//! }
//!
//! impl Encodable for UserInfo {
//!     type Encoded = Vec<u8>;
//!
//!     fn encode(self) -> Self::Encoded {
//!         // Here we define serialization of UserInfo
//! #       todo!()
//!     }
//! }
//!
//! impl Decodable for UserInfo {
//!     fn decode(v: &[u8]) -> anyhow::Result<Self> {
//!         // Here we define deserialization of UserInfo
//! #       todo!()
//!     }
//! }
//!
//! // Define the users table
//! table!(
//!     /// Table with users info.
//!     ( Users ) String => UserInfo
//! );
//!
//! // Assemble database chart
//! pub static TABLES: Lazy<Arc<DatabaseChart>> =
//!     Lazy::new(|| Arc::new([table_info!(Users)].into_iter().collect()));
//!
//! // Create database with the database chart
//! let db = Arc::new(libmdbx::orm::Database::create(&TABLES, None).unwrap());
//!
//! let users = vec![
//!     (
//!         "l33tc0der".to_string(),
//!         UserInfo {
//!             age: 42,
//!             first_name: "Leet".to_string(),
//!             last_name: "Coder".to_string(),
//!         },
//!     ),
//!     (
//!         "lameguy".to_string(),
//!         UserInfo {
//!             age: 25,
//!             first_name: "Lame".to_string(),
//!             last_name: "Guy".to_string(),
//!         },
//!     ),
//! ];
//!
//! let tx = db.begin_readwrite().unwrap();
//!
//! let mut cursor = tx.cursor::<Users>().unwrap();
//!
//! // Insert user info into table
//! for (nickname, user_info) in &users {
//!     cursor.upsert(nickname.clone(), user_info.clone()).unwrap();
//! }
//!
//! // Walk over table and collect its contents
//! assert_eq!(
//!     users,
//!     cursor.walk(None).collect::<anyhow::Result<Vec<_>>>().unwrap()
//! );
//! ```

mod cursor;
mod database;
mod impls;
mod traits;
mod transaction;

pub use self::{cursor::*, database::*, impls::*, traits::*, transaction::*};
pub use crate::{
    dupsort, table, table_info, DatabaseKind, Geometry, NoWriteMap, TransactionKind, WriteMap, RO,
    RW,
};
