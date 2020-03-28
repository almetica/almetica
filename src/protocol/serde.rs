/// Custom serde de/serializer for the TERA network protocol.
mod de;
mod error;
mod ser;

pub use de::{from_vec, Deserializer};
pub use error::{Error, Result};
pub use ser::{to_vec, Serializer};
