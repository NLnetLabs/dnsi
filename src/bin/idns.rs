//! The _idns_ binary.

use clap::Parser;
use domain_tools::idns;

fn main() {
    if let Err(err) = idns::Args::parse().execute() {
        eprintln!("{}", err);
    }
}

