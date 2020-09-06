/// Handles the location of an user.
use crate::model::entity::UserLocation;
use crate::Result;
use anyhow::anyhow;
use nalgebra::{Point3, Rotation3, Vector3};
use sqlx::postgres::PgRow;
use sqlx::prelude::*;
use sqlx::PgConnection;

/// Creates a new user location.
pub async fn create(conn: &mut PgConnection, location: &UserLocation) -> Result<UserLocation> {
    let mut location = sqlx::query(
        r#"INSERT INTO "user_location" VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING *"#,
    )
    .bind(&location.user_id)
    .bind(&location.zone_id)
    .bind(&location.point.x)
    .bind(&location.point.y)
    .bind(&location.point.z)
    .bind(&location.rotation.scaled_axis().x)
    .bind(&location.rotation.scaled_axis().y)
    .bind(&location.rotation.scaled_axis().z)
    .map(map_user)
    .fetch_all(conn)
    .await?;

    location
        .pop()
        .ok_or(anyhow!("Couldn't find the created UserLocation in row"))
}

/// Get the location of a user.
pub async fn get_by_user_id(conn: &mut PgConnection, user_id: i32) -> Result<UserLocation> {
    let mut location = sqlx::query(r#"SELECT * FROM "user_location" WHERE "user_id" = $1"#)
        .bind(&user_id)
        .map(map_user)
        .fetch_all(conn)
        .await?;

    location
        .pop()
        .ok_or(anyhow!("Couldn't find the get UserLocation in row"))
}

/// Updates the location of a user.
pub async fn update(conn: &mut PgConnection, location: &UserLocation) -> Result<UserLocation> {
    let mut location = sqlx::query(
        r#"UPDATE "user_location"
        SET "zone_id" = $1, "location_x" = $2, "location_y" = $3, "location_z" = $4, "rotation_x" = $5, "rotation_y" = $6, "rotation_z" = $7
        WHERE "user_id" = $8
        RETURNING *"#,
        )
        .bind(&location.zone_id)
        .bind(&location.point.x)
        .bind(&location.point.y)
        .bind(&location.point.z)
        .bind(&location.rotation.scaled_axis().x)
        .bind(&location.rotation.scaled_axis().y)
        .bind(&location.rotation.scaled_axis().z)
        .bind(&location.user_id)
        .map(map_user)
        .fetch_all(conn)
        .await?;

    location
        .pop()
        .ok_or(anyhow!("Couldn't find the update UserLocation in row"))
}

fn map_user(row: PgRow) -> UserLocation {
    UserLocation {
        user_id: row.get(0),
        zone_id: row.get(1),
        point: Point3::new(row.get(2), row.get(3), row.get(4)),
        rotation: Rotation3::from_scaled_axis(Vector3::new(row.get(5), row.get(6), row.get(7))),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::model::entity::User;
    use crate::model::repository::account::tests::get_default_account;
    use crate::model::repository::user::tests::get_default_user;
    use crate::model::repository::{account, user};
    use crate::model::tests::db_test;
    use crate::Result;
    use approx::assert_relative_eq;
    use async_std::task;
    use nalgebra::Vector3;
    use sqlx::PgConnection;

    async fn setup(conn: &mut PgConnection) -> Result<User> {
        let account = account::create(conn, &get_default_account(0)).await?;
        user::create(conn, &get_default_user(&account, 0)).await
    }

    #[test]
    fn test_create_user_location() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;
                let user = setup(&mut conn).await?;

                let location = UserLocation {
                    user_id: user.id,
                    zone_id: 14,
                    point: Point3::new(1.0f32, 2.0f32, 3.0f32),
                    rotation: Rotation3::from_axis_angle(&Vector3::z_axis(), 3.0),
                };

                let db_location = create(&mut conn, &location).await?;

                assert_eq!(db_location.user_id, location.user_id);
                assert_eq!(db_location.zone_id, location.zone_id);
                assert_eq!(db_location.point, location.point);
                assert_relative_eq!(db_location.rotation, location.rotation);

                Ok(())
            })
        })
    }

    #[test]
    fn test_get_location_by_user_id() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;
                let user = setup(&mut conn).await?;

                let location = UserLocation {
                    user_id: user.id,
                    zone_id: 14,
                    point: Point3::new(1.0f32, 2.0f32, 3.0f32),
                    rotation: Rotation3::from_axis_angle(&Vector3::z_axis(), 3.0),
                };
                create(&mut conn, &location).await?;

                let db_location = get_by_user_id(&mut conn, user.id).await?;

                assert_eq!(db_location.user_id, location.user_id);
                assert_eq!(db_location.zone_id, location.zone_id);
                assert_eq!(db_location.point, location.point);
                assert_relative_eq!(db_location.rotation, location.rotation);

                Ok(())
            })
        })
    }

    #[test]
    fn test_update_location() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;
                let user = setup(&mut conn).await?;

                let mut location = UserLocation {
                    user_id: user.id,
                    zone_id: 14,
                    point: Point3::new(1.0f32, 2.0f32, 3.0f32),
                    rotation: Rotation3::from_axis_angle(&Vector3::z_axis(), 3.0),
                };
                create(&mut conn, &location).await?;

                location.zone_id = 22;
                location.point = Point3::new(3.0f32, 4.0f32, 5.0f32);
                location.rotation = Rotation3::from_axis_angle(&Vector3::z_axis(), 4.0);
                let db_location = update(&mut conn, &location).await?;

                assert_eq!(db_location.user_id, location.user_id);
                assert_eq!(db_location.zone_id, location.zone_id);
                assert_eq!(db_location.point, location.point);
                assert_relative_eq!(db_location.rotation, location.rotation);

                Ok(())
            })
        })
    }
}
