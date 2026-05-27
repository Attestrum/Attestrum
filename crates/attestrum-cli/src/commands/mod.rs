//! `attestrum` subcommand implementations. Each subcommand owns its own
//! module; the main binary dispatches to `<command>::run(<Args>)`.

pub mod build;
pub mod inspect;
pub mod merge;
pub mod plan;
pub mod prove;
pub mod sign;
pub mod verify;
