/// Module that describes the models used for persistence.
///
/// Only the simple enums and data structures should be shared with the
/// client.
use std::fmt;

use byteorder::{ByteOrder, LittleEndian};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub enum Region {
    International = 0,
    Korea = 1,
    Usa = 2,
    Japan = 3,
    Germany = 4,
    France = 5,
    Europe = 6,
    Taiwan = 7,
    Russia = 8,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub enum Gender {
    Male = 0,
    Female = 1,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub enum Race {
    Human = 0,
    Castanic = 1,
    Aman = 2,
    HighElf = 3,
    ElinPopori = 4,
    Baraka = 5,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub enum Class {
    Warrior = 0,
    Lancer = 1,
    Slayer = 2,
    Berserker = 3,
    Sorcerer = 4,
    Archer = 5,
    Priest = 6,
    Mystic = 7,
    Reaper = 8,
    Gunner = 9,
    Brawler = 10,
    Ninja = 11,
    Valkyrie = 12,
}

pub type Angle = i16;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct Vec3a {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

// type skill_id = [u8; 4]; // Patch < 74
// type skill_id = [u8; 8]; // Path >= 74

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Customization {
    pub data: [u8; 8],
}

impl Serialize for Customization {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(LittleEndian::read_u64(&self.data))
    }
}

impl<'de> Deserialize<'de> for Customization {
    fn deserialize<D>(deserializer: D) -> Result<Customization, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut data: [u8; 8] = [0u8; 8];
        let value = deserializer.deserialize_u64(U64Visitor)?;
        LittleEndian::write_u64(&mut data, value);
        Ok(Customization { data })
    }
}

struct U64Visitor;

impl<'de> Visitor<'de> for U64Visitor {
    type Value = u64;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("8 bytes")
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::serde::{from_vec, to_vec};
    use crate::Result;

    #[test]
    fn test_customization_serialization() -> Result<()> {
        let value = Customization {
            data: [1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8],
        };
        let data = to_vec(&value)?;
        assert_eq!(&value.data, data.as_slice());
        Ok(())
    }

    #[test]
    fn test_customization_deserialization() -> Result<()> {
        let data = vec![1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8];
        let value: Customization = from_vec(data)?;
        assert_eq!([1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8], value.data);
        Ok(())
    }
}
