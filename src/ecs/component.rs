/// Module holds the components that the ECS use.
use crate::model::Region;

/// Tracks the connection information of a user.
pub struct Connection {
    pub verified: bool,
    pub version_checked: bool,
    pub region: Option<Region>,
}
