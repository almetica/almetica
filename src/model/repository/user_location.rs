/// Handles the location of an user.
use crate::model::entity::UserLocation;
use crate::Result;
use nalgebra::{Point3, Rotation3};
use sqlx::prelude::*;
use sqlx::PgConnection;

/// Creates a new user location.
pub async fn create(
    conn: &mut PgConnection,
    user_id: i32,
    zone: i32,
    point: &Point3<f32>,
    rotation: &Rotation3<f32>,
) -> Result<UserLocation> {
    Ok(sqlx::query_as::<_, UserLocation>(
        r#"INSERT INTO "user_location" VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING *"#,
    )
    .bind(&user_id)
    .bind(&zone)
    .bind(&point.x)
    .bind(&point.y)
    .bind(&point.z)
    .bind(&rotation.scaled_axis().x)
    .bind(&rotation.scaled_axis().y)
    .bind(&rotation.scaled_axis().z)
    .fetch_one(conn)
    .await?)
}

/// Updates the location of a user.
pub async fn update(
    conn: &mut PgConnection,
    user_id: i32,
    zone: i32,
    point: &Point3<f32>,
    rotation: &Rotation3<f32>,
) -> Result<UserLocation> {
    Ok(sqlx::query_as::<_, UserLocation>(
        r#"UPDATE "user_location" 
        SET "zone" = $1, "location_x" = $2, "location_y" = $3, "location_z" = $4, "rotation_x" = $5, "rotation_y" = $6, "rotation_z" = $7
        WHERE "user_id" = $8
        RETURNING *"#,
    )
    .bind(&zone)
    .bind(&point.x)
    .bind(&point.y)
    .bind(&point.z)
    .bind(&rotation.scaled_axis().x)
    .bind(&rotation.scaled_axis().y)
    .bind(&rotation.scaled_axis().z)
    .bind(&user_id)
    .fetch_one(conn)
    .await?)
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

                let zone: i32 = 14;
                let point = Point3::new(1.0f32, 2.0f32, 3.0f32);
                let rotation = Rotation3::from_axis_angle(&Vector3::z_axis(), 3.0);

                let db_location = create(&mut conn, user.id, zone, &point, &rotation).await?;

                assert_eq!(db_location.user_id, user.id);
                assert_eq!(db_location.zone, zone);
                assert_eq!(db_location.location_x, point.x);
                assert_eq!(db_location.location_y, point.y);
                assert_eq!(db_location.location_z, point.z);
                assert_eq!(db_location.rotation_x, rotation.scaled_axis().x);
                assert_eq!(db_location.rotation_y, rotation.scaled_axis().y);
                assert_eq!(db_location.rotation_z, rotation.scaled_axis().z);

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

                let zone: i32 = 14;
                let point = Point3::new(1.0f32, 2.0f32, 3.0f32);
                let rotation = Rotation3::from_axis_angle(&Vector3::z_axis(), 3.0);
                create(&mut conn, user.id, zone, &point, &rotation).await?;

                let new_point = Point3::new(3.0f32, 4.0f32, 5.0f32);
                let new_location = update(&mut conn, user.id, zone, &new_point, &rotation).await?;

                assert_eq!(new_location.user_id, user.id);
                assert_eq!(new_location.zone, zone);
                assert_eq!(new_location.location_x, new_point.x);
                assert_eq!(new_location.location_y, new_point.y);
                assert_eq!(new_location.location_z, new_point.z);
                assert_eq!(new_location.rotation_x, rotation.scaled_axis().x);
                assert_eq!(new_location.rotation_y, rotation.scaled_axis().y);
                assert_eq!(new_location.rotation_z, rotation.scaled_axis().z);

                Ok(())
            })
        })
    }
}
