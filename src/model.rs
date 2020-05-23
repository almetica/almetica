/// Module that abstracts the persistence model.
pub mod entity;
pub mod migrations;
pub mod repository;

use byteorder::{ByteOrder, LittleEndian};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type, PartialEq)]
#[sqlx(rename = "gender")]
pub enum Gender {
    #[sqlx(rename = "male")]
    Male = 0,
    #[sqlx(rename = "female")]
    Female = 1,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type, PartialEq)]
#[sqlx(rename = "race")]
pub enum Race {
    #[sqlx(rename = "human")]
    Human = 0,
    #[sqlx(rename = "castanic")]
    Castanic = 1,
    #[sqlx(rename = "aman")]
    Aman = 2,
    #[sqlx(rename = "high elf")]
    HighElf = 3,
    #[sqlx(rename = "elin popori")]
    ElinPopori = 4,
    #[sqlx(rename = "baraka")]
    Baraka = 5,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type, PartialEq)]
#[sqlx(rename = "user_class")]
pub enum Class {
    #[sqlx(rename = "warrior")]
    Warrior = 0,
    #[sqlx(rename = "lancer")]
    Lancer = 1,
    #[sqlx(rename = "slayer")]
    Slayer = 2,
    #[sqlx(rename = "berserker")]
    Berserker = 3,
    #[sqlx(rename = "sorcerer")]
    Sorcerer = 4,
    #[sqlx(rename = "archer")]
    Archer = 5,
    #[sqlx(rename = "priest")]
    Priest = 6,
    #[sqlx(rename = "elementalist")]
    Elementalist = 7,
    #[sqlx(rename = "soulless")]
    Soulless = 8,
    #[sqlx(rename = "engineer")]
    Engineer = 9,
    #[sqlx(rename = "fighter")]
    Fighter = 10,
    #[sqlx(rename = "ninja")]
    Ninja = 11,
    #[sqlx(rename = "valkyrie")]
    Valkyrie = 12,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type, PartialEq)]
#[sqlx(rename = "servant_type")]
pub enum ServantType {
    #[sqlx(rename = "pet")]
    Pet = 0,
    #[sqlx(rename = "partner")]
    Partner = 1,
}

/// TERA transmits the rotation of objects as a u16 value. It's a fraction value of a full rotation.
/// 0x0 = 0°, 0xFFFF = 360°
#[derive(Clone, Copy, Debug, sqlx::Type, PartialEq)]
#[sqlx(transparent)]
pub struct Angle(u16);

impl Angle {
    pub fn from_deg(deg: f32) -> Self {
        Angle((deg as f32 * 0x10000 as f32 / 360 as f32) as u16)
    }

    pub fn raw(&self) -> u16 {
        self.0
    }

    pub fn rad(&self) -> f32 {
        self.0 as f32 * (2.0 * std::f32::consts::PI) / 0x10000 as f32
    }

    pub fn deg(&self) -> f32 {
        self.0 as f32 * 360.0 / 0x10000 as f32
    }

    pub fn normalize(&self) -> Self {
        Angle(((self.0 as u32 + 0x8000) % 0x10000 - 0x8000) as u16)
    }
}

impl Default for Angle {
    fn default() -> Self {
        Angle(0)
    }
}

impl fmt::Display for Angle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}°", self.deg())
    }
}

impl Serialize for Angle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(self.0)
    }
}

impl<'de> Deserialize<'de> for Angle {
    fn deserialize<D>(deserializer: D) -> Result<Angle, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = deserializer.deserialize_u16(U16Visitor)?;
        Ok(Angle(value))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for Vec3 {
    fn default() -> Self {
        Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type, PartialEq)]
pub struct Vec3a {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Default for Vec3a {
    fn default() -> Self {
        Vec3a { x: 0, y: 0, z: 0 }
    }
}

// type skill_id = [u8; 4]; // Patch < 74
// type skill_id = [u8; 8]; // Path >= 74

#[derive(Clone, Debug, sqlx::Type, PartialEq)]
#[sqlx(transparent)]
pub struct Customization(pub Vec<u8>);

impl Default for Customization {
    fn default() -> Self {
        Customization(vec![0, 0, 0, 0, 0, 0, 0, 0])
    }
}

impl Serialize for Customization {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(LittleEndian::read_u64(&self.0))
    }
}

impl<'de> Deserialize<'de> for Customization {
    fn deserialize<D>(deserializer: D) -> Result<Customization, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut data = vec![0u8; 8];
        let value = deserializer.deserialize_u64(U64Visitor)?;
        LittleEndian::write_u64(&mut data, value);
        Ok(Customization(data))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateID {
    pub race: Race,
    pub gender: Gender,
    pub class: Class,
}

impl Default for TemplateID {
    fn default() -> Self {
        TemplateID {
            race: Race::Human,
            gender: Gender::Male,
            class: Class::Warrior,
        }
    }
}

impl Serialize for TemplateID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i32(
            10101 + ((self.race as i32 * 2 + self.gender as i32) * 100) + self.class as i32,
        )
    }
}

impl<'de> Deserialize<'de> for TemplateID {
    fn deserialize<D>(deserializer: D) -> Result<TemplateID, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut value = deserializer.deserialize_i32(I32Visitor)? - 10101;

        let gender = if value % 200 >= 100 {
            value -= 100;
            Gender::Female
        } else {
            Gender::Male
        };

        let race = match value / 200 {
            0 => Race::Human,
            1 => Race::Castanic,
            2 => Race::Aman,
            3 => Race::HighElf,
            4 => Race::ElinPopori,
            5 => Race::Baraka,
            _ => {
                return Err(de::Error::custom(format!(
                    "unknown race when parsing TemplateID {}",
                    value
                )))
            }
        };

        let class = match value % 100 {
            0 => Class::Warrior,
            1 => Class::Lancer,
            2 => Class::Slayer,
            3 => Class::Berserker,
            4 => Class::Sorcerer,
            5 => Class::Archer,
            6 => Class::Priest,
            7 => Class::Elementalist,
            8 => Class::Soulless,
            9 => Class::Engineer,
            10 => Class::Fighter,
            11 => Class::Ninja,
            12 => Class::Valkyrie,
            _ => {
                return Err(de::Error::custom(format!(
                    "unknown class when parsing TemplateID {}",
                    value
                )))
            }
        };

        Ok(TemplateID {
            race,
            gender,
            class,
        })
    }
}

/// Supported password hash algorithms.
#[derive(Clone, Debug, sqlx::Type, PartialEq)]
#[sqlx(rename = "password_hash_algorithm")]
pub enum PasswordHashAlgorithm {
    #[sqlx(rename = "argon2")]
    Argon2,
}

struct U16Visitor;

impl<'de> Visitor<'de> for U16Visitor {
    type Value = u16;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("2 bytes")
    }

    fn visit_u16<E>(self, value: u16) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(value)
    }
}

struct I32Visitor;

impl<'de> Visitor<'de> for I32Visitor {
    type Value = i32;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("4 bytes")
    }

    fn visit_i32<E>(self, value: i32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(value)
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
pub mod tests {
    use super::*;
    use crate::model::migrations;
    use crate::protocol::serde::{from_vec, to_vec};
    use crate::Result;
    use anyhow::Context;
    use async_std::task;
    use hex::encode;
    use rand::{thread_rng, RngCore};
    use sqlx::{Connect, PgConnection};
    use std::panic;

    /// Executes a test with a database connection. Prepares a new test database that is cleaned up after the test.
    /// Configure the TEST_DATABASE_CONNECTION in your .env file. The user needs to have access to the postgres database
    /// and have the permission to create / delete databases.
    pub fn db_test<'a, F>(test: F) -> Result<()>
    where
        F: FnOnce(&str) -> Result<()> + panic::UnwindSafe,
    {
        let _ = dotenv::dotenv();
        let db_url = &dotenv::var("TEST_DATABASE_CONNECTION")?;

        let db_name = task::block_on(async { setup_db(db_url).await })?;

        // Don't re-use the connection when testing. It could get tainted.
        let result = panic::catch_unwind(|| {
            let db_string = format!("{}/{}", db_url, db_name);
            if let Err(e) = test(&db_string) {
                panic!("Error while executing test: {:?}", e);
            }
        });

        task::block_on(async { teardown_db(db_url, &db_name).await })?;

        assert!(result.is_ok());
        Ok(())
    }

    /// Creates a randomly named test database.
    async fn setup_db(db_url: &str) -> Result<String> {
        let mut random = vec![0u8; 28];
        thread_rng().fill_bytes(random.as_mut_slice());
        let db_name: String = format!("test_{}", encode(random));

        let mut conn = PgConnection::connect(format!("{}/postgres", db_url)).await?;
        sqlx::query(format!("CREATE DATABASE {};", db_name).as_ref())
            .execute(&mut conn)
            .await?;

        // Run migrations on the temporary database
        migrations::apply(db_url, &db_name)
            .await
            .context("Can't migrate database schema")?;

        Ok(db_name)
    }

    /// Deletes the randomly named test database.
    async fn teardown_db(db_url: &str, db_name: &str) -> Result<()> {
        let mut conn = PgConnection::connect(format!("{}/postgres", db_url)).await?;

        // Drop all other connections to the database
        sqlx::query(
            format!(
                r#"SELECT pg_terminate_backend(pg_stat_activity.pid)
                   FROM pg_stat_activity
                   WHERE datname = '{}'
                   AND pid <> pg_backend_pid();"#,
                db_name
            )
            .as_ref(),
        )
        .execute(&mut conn)
        .await?;

        // Drop the database itself
        sqlx::query(format!("DROP DATABASE {}", db_name).as_ref())
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    #[test]
    fn test_customization_serialization() -> Result<()> {
        let value = Customization(vec![1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8]);
        let data = to_vec(&value)?;
        assert_eq!(&data, &value.0);
        Ok(())
    }

    #[test]
    fn test_customization_deserialization() -> Result<()> {
        let data = vec![1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8];
        let value: Customization = from_vec(data)?;
        assert_eq!(value.0, [1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8]);
        Ok(())
    }

    #[test]
    fn test_template_id_serialization_elin_lancer() -> Result<()> {
        let value = TemplateID {
            race: Race::ElinPopori,
            gender: Gender::Female,
            class: Class::Lancer,
        };
        let data = to_vec(&value)?;
        assert_eq!(LittleEndian::read_i32(&data), 11002);
        Ok(())
    }

    #[test]
    fn test_template_id_serialization_human_sorcerer() -> Result<()> {
        let value = TemplateID {
            race: Race::Human,
            gender: Gender::Male,
            class: Class::Sorcerer,
        };
        let data = to_vec(&value)?;
        assert_eq!(LittleEndian::read_i32(&data), 10105);
        Ok(())
    }

    #[test]
    fn test_template_id_deserialization_elin_lancer() -> Result<()> {
        let mut data = vec![0u8; 4];
        LittleEndian::write_i32(&mut data, 11002);
        let value: TemplateID = from_vec(data)?;
        assert_eq!(value.race, Race::ElinPopori);
        assert_eq!(value.gender, Gender::Female);
        assert_eq!(value.class, Class::Lancer);
        Ok(())
    }

    #[test]
    fn test_template_id_deserialization_human_sorcerer() -> Result<()> {
        let mut data = vec![0u8; 4];
        LittleEndian::write_i32(&mut data, 10105);
        let value: TemplateID = from_vec(data)?;
        assert_eq!(value.race, Race::Human);
        assert_eq!(value.gender, Gender::Male);
        assert_eq!(value.class, Class::Sorcerer);
        Ok(())
    }

    #[test]
    fn test_angle_basic() {
        for i in 0..360 {
            assert_eq!(Angle::from_deg(i as f32).deg().round(), (i as f32).round());
        }
        assert_eq!(Angle::from_deg(360.0).deg(), 0.0);
    }

    #[test]
    fn test_angle_serialization() -> Result<()> {
        let value = Angle::from_deg(180.0);
        let data = to_vec(&value)?;
        assert_eq!(LittleEndian::read_u16(&data), 32768);
        Ok(())
    }

    #[test]
    fn test_angle_deserialization() -> Result<()> {
        let mut data = vec![0u8; 2];
        LittleEndian::write_u16(&mut data, 32768);
        let value: Angle = from_vec(data)?;
        assert_eq!(value.deg(), 180.0);
        Ok(())
    }
}
