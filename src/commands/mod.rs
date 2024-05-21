//! The various commands of _idns._

pub mod help;
pub mod query;
pub mod lookup;


use super::error::Error;


#[derive(Clone, Debug, clap::Subcommand)]
pub enum Commands {
    /// Query the DNS.
    Query(self::query::Query),

    /// Lookup a host or address.
    Lookup(self::lookup::Lookup),

    /// Show the manual pages.
    Man(self::help::Help),
}

impl Commands {
    pub fn execute(self) -> Result<(), Error> {
        match self {
            Self::Query(query) => query.execute(),
            Self::Lookup(lookup) => lookup.execute(),
            Self::Man(help) => help.execute(),
        }
    }
}

