//! The _dnsi_ binary.

use clap::Parser;
use domain_tools::dnsi;

fn main() {
    if let Err(err) = dnsi::Args::parse().execute() {
        eprintln!("{}", err);
    }
}

