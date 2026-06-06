//! `attestrum` subcommand implementations. Each subcommand owns its own
//! module; the main binary dispatches to `<command>::run(<Args>)`.

pub mod bind;
pub mod build;
pub mod index;
pub mod inspect;
pub mod merge;
pub mod oidc;
pub mod plan;
pub mod prove;
pub mod publish;
pub mod sign;
pub mod verify;
pub mod walk_chain;
