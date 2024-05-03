//! The _dnsi_ binary.

use clap::Parser;

fn main() {
    if let Err(err) = dnsi::Args::parse().execute() {
        eprintln!("{}", err);
    }
}

