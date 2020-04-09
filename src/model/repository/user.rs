/// Handles the user of a player (the characters).
use mysql::prelude::*;

use crate::Result;

/// Creates a new user.
pub fn create<Q>(mut _conn: Q) -> Result<()>
where
    Q: Queryable,
{
    Ok(())
}

/// Updates a user.
pub fn update<Q>(mut _conn: Q) -> Result<()>
where
    Q: Queryable,
{
    Ok(())
}

/// Finds a user by ID.
pub fn get_by_id<Q>(mut _conn: Q, _id: u64) -> Result<()>
where
    Q: Queryable,
{
    Ok(())
}

/// Deletes a user with the given ID.
pub fn delete<Q>(mut _conn: Q, _id: u64) -> Result<()>
where
    Q: Queryable,
{
    Ok(())
}
