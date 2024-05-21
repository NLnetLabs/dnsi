//! Global configuration.

use super::commands::Commands;
use super::error::Error;


#[derive(Clone, Debug, clap::Parser)]
#[command(disable_help_subcommand = true)]
pub struct Args {
    #[command(subcommand)]
    command: Commands,
}

impl Args {
    pub fn execute(self) -> Result<(), Error> {
        self.command.execute()
    }
}

