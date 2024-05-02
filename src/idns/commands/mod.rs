//! The various commands of _idsn._

pub mod help;
pub mod query;


use super::error::Error;


#[derive(Clone, Debug, clap::Subcommand)]
pub enum Commands {
    /// Query the DNS.
    Query(self::query::Query),

    /// Show the manual pages.
    Man(self::help::Help),
}

impl Commands {
    pub fn execute(self) -> Result<(), Error> {
        match self {
            Self::Query(query) => query.execute(),
            Self::Man(help) => help.execute(),
        }
    }
}

