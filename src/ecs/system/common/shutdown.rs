use crate::ecs::resource::{ShutdownSignal, ShutdownSignalStatus};
use shipyard::*;
use tracing::info;

// TODO test the shutdown system

/// Simple system that sets the finished the shutdown signal. Later ECS iteration may make this switch once they know a shutdown has been properly handled.
pub fn shutdown_system(mut shutdown: UniqueViewMut<ShutdownSignal>) {
    if shutdown.status == ShutdownSignalStatus::ShutdownInProgress {
        info!("Setting shutdown signal to status ShutdownSignalStatus::Shutdown");
        shutdown.status = ShutdownSignalStatus::Shutdown;
    }
}
