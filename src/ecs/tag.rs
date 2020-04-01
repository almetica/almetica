use crate::ecs::event;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventKind(pub event::EventKind);
