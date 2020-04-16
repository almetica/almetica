/// Module that abstracts the persistence model.
pub mod entity;
pub mod repository;
pub mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("./src/model/migrations");
}

use std::fmt;

use byteorder::{ByteOrder, LittleEndian};
use postgres_types::{FromSql, ToSql};
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
    Elementalist = 7,
    Soulless = 8,
    Engineer = 9,
    Fighter = 10,
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

/// Supported password hash algorithms.
#[derive(Debug, FromSql, ToSql, PartialEq)]
#[postgres(name = "password_hash_algorithm")]
pub enum PasswordHashAlgorithm {
    #[postgres(name = "argon2")]
    Argon2,
}

#[cfg(test)]
pub mod tests {
    use std::panic;

    use hex::encode;
    use postgres::{Client, Config, NoTls};
    use rand::{thread_rng, RngCore};

    use crate::model::embedded::migrations;
    use crate::protocol::serde::{from_vec, to_vec};
    use crate::{Result, SyncDbPool};

    use super::*;

    /// Executes a test with a database connection. Prepares a new test database that is cleaned up after the test.
    /// Configure the DATABASE_CONNECTION in your .env file. The user needs to have access to the postgres database
    /// and have the permission to create / delete databases.
    pub fn db_test<T>(test: T) -> Result<()>
    where
        T: FnOnce(SyncDbPool) -> Result<()> + panic::UnwindSafe,
    {
        // Read and assemble to database connection configuration
        let _ = dotenv::dotenv();
        let db_url = &dotenv::var("DATABASE_CONNECTION")?;
        let mut config: Config = db_url.parse()?;

        let (client, db_name) = setup_db(config.clone())?;

        // Don't re-use the connection when testing. It could get tainted.
        let result = panic::catch_unwind(|| {
            config.dbname(&db_name);
            let manager = r2d2_postgres::PostgresConnectionManager::new(config, NoTls);
            let pool = r2d2::Pool::builder().max_size(1).build(manager).unwrap();
            test(pool).unwrap()
        });

        teardown_db(client, db_name)?;

        Ok(assert!(result.is_ok()))
    }

    /// Creates a randomly named test database.
    fn setup_db(mut config: Config) -> Result<(Client, String)> {
        let mut random = vec![0u8; 32];
        thread_rng().fill_bytes(random.as_mut_slice());
        let db_name: String = format!("test_{}", encode(random));

        config.dbname("postgres");
        let mut client = config.connect(NoTls)?;
        client.batch_execute(format!("CREATE DATABASE {};", db_name).as_ref())?;

        // Run migrations on the temporary database
        config.dbname(&db_name);
        let mut migration_client = config.connect(NoTls)?;
        migrations::runner().run(&mut migration_client)?;

        Ok((client, db_name))
    }

    /// Deletes the randomly named test database.
    fn teardown_db(mut client: Client, db_name: String) -> Result<()> {
        // Drop all other connections to the database. It seems that either the pool
        // or the postgres tokio runtime doesn't close all connections on drop()...
        client.batch_execute(
            format!(
                r#"
                SELECT pg_terminate_backend(pg_stat_activity.pid)
                FROM pg_stat_activity
                WHERE datname = '{}'
                AND pid <> pg_backend_pid();
                "#,
                &db_name
            )
            .as_ref(),
        )?;
        // Drop the database itself
        client.batch_execute(
            format!(
                r#"
                DROP DATABASE {};
                "#,
                &db_name
            )
            .as_ref(),
        )?;
        Ok(())
    }

    #[test]
    fn test_customization_serialization() -> Result<()> {
        let value = Customization {
            data: [1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8],
        };
        let data = to_vec(&value)?;
        assert_eq!(data.as_slice(), &value.data);
        Ok(())
    }

    #[test]
    fn test_customization_deserialization() -> Result<()> {
        let data = vec![1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8];
        let value: Customization = from_vec(data)?;
        assert_eq!(value.data, [1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8]);
        Ok(())
    }
}
