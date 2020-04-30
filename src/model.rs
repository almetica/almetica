/// Module that abstracts the persistence model.
pub mod entity;
pub mod repository;
pub mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("./src/model/migrations");
}

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

pub type Angle = i16;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type, PartialEq)]
pub struct Vec3a {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

// type skill_id = [u8; 4]; // Patch < 74
// type skill_id = [u8; 8]; // Path >= 74

#[derive(Clone, Debug, sqlx::Type, PartialEq)]
pub struct Customization {
    pub data: Vec<u8>,
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
        let mut data = vec![0u8; 8];
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
#[derive(Debug, sqlx::Type, PartialEq)]
#[sqlx(rename = "password_hash_algorithm")]
pub enum PasswordHashAlgorithm {
    #[sqlx(rename = "argon2")]
    Argon2,
}

#[cfg(test)]
pub mod tests {
    use std::panic;

    use async_std::task;
    use hex::encode;
    use rand::{thread_rng, RngCore};
    use sqlx::{Connect, PgConnection, PgPool};
    use std::future::Future;
    use tokio::runtime::Runtime;

    use crate::model::embedded::migrations;
    use crate::protocol::serde::{from_vec, to_vec};
    use crate::Result;

    use super::*;

    /// Executes a test with a database connection. Prepares a new test database that is cleaned up after the test.
    /// Configure the DATABASE_CONNECTION in your .env file. The user needs to have access to the postgres database
    /// and have the permission to create / delete databases.
    /// Uses the default async_std runtime. Just use the standard `[test]` macro.
    pub fn db_test<'a, T, F>(test: F) -> Result<()>
    where
        T: Future<Output = Result<()>> + 'a,
        F: FnOnce(PgPool) -> T + panic::UnwindSafe,
    {
        let _ = dotenv::dotenv();
        let db_url = &dotenv::var("TEST_DATABASE_CONNECTION")?;

        // FIXME: Switch to pure sqlx once refinery added support for it.
        let mut db_name = "".to_string();
        {
            let mut config: tokio_postgres::Config = db_url.parse()?;
            config.dbname("postgres");

            let mut rt = Runtime::new()?;
            rt.block_on(async {
                db_name = setup_db(config.clone()).await.unwrap();
            });
        }

        // Don't re-use the connection when testing. It could get tainted.
        let result = panic::catch_unwind(|| {
            task::block_on(async {
                let db_string = format!("{}/{}", db_url, db_name);
                let pool = PgPool::new(&db_string).await.unwrap();
                if let Err(e) = test(pool).await {
                    panic!("Error while executing test: {}", e);
                }
            });
        });

        task::block_on(async {
            teardown_db(format!("{}/postgres", db_url).as_ref(), db_name)
                .await
                .unwrap();
        });

        assert!(result.is_ok());
        Ok(())
    }

    /// Creates a randomly named test database.
    async fn setup_db(mut config: tokio_postgres::Config) -> Result<String> {
        let mut random = vec![0u8; 32];
        thread_rng().fill_bytes(random.as_mut_slice());
        let db_name: String = format!("test_{}", encode(random));

        let (client, connection) = config.connect(tokio_postgres::NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        client
            .batch_execute(format!("CREATE DATABASE {};", db_name).as_ref())
            .await?;

        // Run migrations on the temporary database
        config.dbname(&db_name);
        let (mut migration_client, connection) = config.connect(tokio_postgres::NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        migrations::runner()
            .run_async(&mut migration_client)
            .await?;

        Ok(db_name)
    }

    /// Deletes the randomly named test database.
    async fn teardown_db(db_url: &str, db_name: String) -> Result<()> {
        let mut conn = PgConnection::connect(db_url).await?;

        // Drop all other connections to the database
        sqlx::query(
            format!(
                r#"SELECT pg_terminate_backend(pg_stat_activity.pid)
                   FROM pg_stat_activity
                   WHERE datname = '{}'
                   AND pid <> pg_backend_pid();"#,
                &db_name
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
        let value = Customization {
            data: vec![1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8],
        };
        let data = to_vec(&value)?;
        assert_eq!(&data, &value.data);
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
