/// Custom serde de/serializer for the Tera network protocol.
mod de;
mod error;
mod ser;

pub use de::{from_vec, Deserializer};
pub use error::{Error, Result};
//pub use ser::Serializer;
