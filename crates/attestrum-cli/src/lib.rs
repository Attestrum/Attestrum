//! `attestrum-cli` crate library. The `[[bin]] attestrum` target consumes
//! everything here through `use attestrum_cli::*`; integration tests under
//! `tests/` consume the same surface.
//!
//! Public module layout:
//!
//! - [`commands`] — per-subcommand implementations (`build` from E5,
//!   `inspect` from E6).
//! - [`lifecycle`] — pure state machines that describe subcommand flow
//!   without doing any I/O. The `attestrum inspect` lifecycle lives here
//!   alongside its diagram at `docs/diagrams/sprint-3/attestrum-inspect-lifecycle.md`.

pub mod commands;
pub mod lifecycle;
