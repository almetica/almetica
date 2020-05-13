/// Custom SQL migration toolkit. The code is implicitly tested by the other model codebase.
use crate::Result;
use anyhow::{bail, ensure, Context};
use rust_embed::RustEmbed;
use sqlx::pool::PoolConnection;
use sqlx::postgres::PgRow;
use sqlx::{Connect, Executor, PgConnection, PgPool, Row};
use std::borrow::Cow;
use std::str;
use tracing::{info, warn};

#[derive(RustEmbed)]
#[folder = "src/model/migrations/"]
struct MigrationFiles;

pub async fn apply(db_url: &str, db_name: &str) -> Result<()> {
    let migrator = Migrator {
        db_url: db_url.to_string(),
        db_name: db_name.to_string(),
    };
    migrator.create_migration_table().await?;

    if !migrator.database_exists().await? {
        migrator.create_database().await?;
    }

    info!("Checking lineage of the migration lists");
    let mut new_migrations: Vec<Cow<'static, str>> = MigrationFiles::iter().collect();
    let mut applied_migrations = migrator.get_migrations().await?;

    new_migrations.sort();
    applied_migrations.sort();

    ensure!(
        applied_migrations.len() <= new_migrations.len(),
        "New migration list is smaller than applied migration list"
    );

    // Make sure that we are in the same lineage with the migrations.
    if new_migrations.len() > applied_migrations.len() {
        for (i, a_migration) in applied_migrations.iter().enumerate() {
            if let Some(n_migration) = new_migrations.get(i) {
                ensure!(
                    a_migration == n_migration,
                    format!(
                        "Applied migration can't be found in the expected lineage location in new migration list: {}",
                        a_migration
                    )
                );
            } else {
                bail!("Can't find new migration on position: {}", i);
            }
        }
    }

    for migration_file_name in new_migrations.iter().skip(applied_migrations.len()) {
        info!("Applying migration: {}", migration_file_name);
        let data = MigrationFiles::get(&migration_file_name).unwrap();
        let migration_sql = str::from_utf8(&data)?;

        let mut migration = migrator.begin_migration().await?;
        match apply_migration_file(&migration_file_name, migration_sql, &mut migration).await {
            Ok(..) => {
                migration.commit().await?;
            }
            Err(e) => {
                migration.rollback().await?;
                bail!(
                    "Failed to apply migration file {}: {}",
                    &migration_file_name,
                    e
                );
            }
        }
    }

    Ok(())
}

async fn apply_migration_file(
    migration_file_name: &str,
    migration_sql: &str,
    migration: &mut Migration,
) -> Result<()> {
    if !migration.is_applied(migration_file_name).await? {
        migration.execute_migration(migration_sql).await?;
        migration
            .save_applied_migration(migration_file_name)
            .await?;
    } else {
        warn!("Migration {} is already applied!", migration_file_name);
    }
    Ok(())
}

struct Migrator {
    db_url: String,
    db_name: String,
}

impl Migrator {
    async fn create_database(&self) -> Result<()> {
        let mut conn = PgConnection::connect(format!("{}/postgres", self.db_url)).await?;

        info!("Creating database {}", &self.db_name);
        sqlx::query(&format!(r#"CREATE DATABASE {}"#, &self.db_name))
            .execute(&mut conn)
            .await
            .with_context(|| format!("Failed to create database: {}", &self.db_name))?;

        Ok(())
    }

    async fn create_migration_table(&self) -> Result<()> {
        let mut conn = PgConnection::connect(format!("{}/{}", self.db_url, self.db_name)).await?;

        info!("Checking for migration table");
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS "migration" (
                    "version" BIGINT PRIMARY KEY,
                    "name" TEXT NOT NULL,
                    "created_at" TIMESTAMP WITH TIME ZONE DEFAULT current_timestamp
                );"#,
        )
        .execute(&mut conn)
        .await
        .context("Failed to create migration table")?;

        Ok(())
    }

    async fn database_exists(&self) -> Result<bool> {
        let mut conn = PgConnection::connect(format!("{}/postgres", self.db_url)).await?;

        info!("Checking if database {} exists", &self.db_name);
        Ok(sqlx::query(
            r#"SELECT EXISTS(SELECT 1 FROM "pg_database" WHERE "datname" = $1) AS exists"#,
        )
        .bind(&self.db_name)
        .try_map(|row: PgRow| row.try_get("exists"))
        .fetch_one(&mut conn)
        .await
        .context("Failed to check if database exists")?)
    }

    async fn get_migrations(&self) -> Result<Vec<String>> {
        let mut conn = PgConnection::connect(format!("{}/{}", self.db_url, self.db_name)).await?;

        Ok(
            sqlx::query(r#"SELECT "version", "name" FROM "migration" ORDER BY "version""#)
                .try_map(|row: PgRow| {
                    let version: i64 = row.try_get(0)?;
                    let migration_name: String = row.try_get(1)?;
                    Ok(format!("{}__{}", version, migration_name))
                })
                .fetch_all(&mut conn)
                .await
                .context("Failed to query migration table")?,
        )
    }

    async fn begin_migration(&self) -> Result<Box<Migration>> {
        let pool = PgPool::new(format!("{}/{}", self.db_url, self.db_name).as_ref()).await?;

        Ok(Box::new(Migration {
            transaction: pool.begin().await?,
        }))
    }
}

struct Migration {
    transaction: sqlx::Transaction<PoolConnection<PgConnection>>,
}

impl Migration {
    async fn commit(self: Box<Self>) -> Result<()> {
        self.transaction.commit().await?;

        Ok(())
    }

    async fn rollback(self: Box<Self>) -> Result<()> {
        self.transaction.rollback().await?;

        Ok(())
    }

    async fn is_applied(&mut self, migration_file_name: &str) -> Result<bool> {
        let (version, migration_name) = self.parse_migration_version_name(migration_file_name)?;

        Ok(sqlx::query(
            r#"SELECT EXISTS(SELECT 1 FROM "migration" WHERE "version"= $1 AND "name" = $2) AS exists"#,
        )
            .bind(version)
            .bind(migration_name.to_string())
            .try_map(|row: PgRow| row.try_get("exists"))
            .fetch_one(&mut self.transaction)
            .await
            .context("Failed to check migration table")?)
    }

    async fn execute_migration(&mut self, migration_sql: &str) -> Result<()> {
        self.transaction.execute(migration_sql).await?;

        Ok(())
    }

    async fn save_applied_migration(&mut self, migration_file_name: &str) -> Result<()> {
        let (version, migration_name) = self.parse_migration_version_name(migration_file_name)?;

        sqlx::query(r#"INSERT INTO "migration" VALUES ($1, $2, DEFAULT)"#)
            .bind(version)
            .bind(migration_name.to_string())
            .execute(&mut self.transaction)
            .await
            .context("Failed to insert migration")?;

        Ok(())
    }

    fn parse_migration_version_name(&self, migration_file_name: &str) -> Result<(i64, String)> {
        let split: Vec<&str> = migration_file_name.split("__").collect();
        ensure!(split.len() == 2, "Incompatible migration file name. Needs to be: %VERSION_NUMBER%__%MIGRATION_NAME_STRING%.sql");
        let version: i64 = split[0]
            .parse()
            .context("VERSION_NUMBER is not a valid i64")?;
        let migration_name = split[1];

        Ok((version, migration_name.to_string()))
    }
}
