//! The actual implementation of _dnsi._

pub use self::args::Args;

pub mod args;
pub mod client;
pub mod commands;
pub mod error;
pub mod output;
