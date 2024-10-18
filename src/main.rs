//! The _dnsi_ binary.

use clap::Parser;
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_thread_ids(true)
        .without_time()
        .try_init()
        .ok();

    if let Err(err) = dnsi::Args::parse().execute() {
        eprintln!("{}", err);
    }
}
