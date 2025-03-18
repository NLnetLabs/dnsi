//! The various commands of _idns._

pub mod help;
pub mod lookup;
pub mod query;
pub mod xfr;

use super::error::Error;

#[derive(Clone, Debug, clap::Subcommand)]
pub enum Command {
    /// Query the DNS.
    Query(self::query::Query),

    /// Lookup a host or address.
    Lookup(self::lookup::Lookup),

    /// Transfer a zone.
    Xfr(self::xfr::Xfr),

    /// Show the manual pages.
    Help(self::help::Help),
}

impl Command {
    pub fn execute(self) -> Result<(), Error> {
        match self {
            Self::Query(query) => query.execute(),
            Self::Lookup(lookup) => lookup.execute(),
            Self::Xfr(xfr) => xfr.execute(),
            Self::Help(help) => help.execute(),
        }
    }
}
