//! The help command of _idns._

use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;
use crate::idns::error::Error;


//------------ Help ----------------------------------------------------------

#[derive(Clone, Debug, clap::Args)]
pub struct Help {
    /// The command to show the man page for
    #[arg(value_name="COMMAND")]
    command: Option<String>,
}

impl Help {
    pub fn execute(self) -> Result<(), Error> {
        let page = match self.command.as_deref() {
            None => Self::IDNS_1,
            Some("query") => Self::IDNS_QUERY_1,
            Some(command) => {
                return Err(format!("Unknown command '{}'.", command).into());
            }
        };

        let mut file = NamedTempFile::new().map_err(|err| {
            format!(
                "Can't display man page: \
                 Failed to create temporary file: {}.",
                err
            )
        })?;
        file.write_all(page).map_err(|err| {
            format!(
                "Can't display man page: \
                Failed to write to temporary file: {}.",
                err
            )
        })?;
        let _ = Command::new("man").arg(file.path()).status().map_err(|err| {
            format!("Failed to run man: {}", err)
        })?;
        Ok(())
    }
}

impl Help {
    const IDNS_1: &'static [u8] = include_bytes!("../../../doc/idns.1");
    const IDNS_QUERY_1: &'static [u8] = include_bytes!(
        "../../../doc/idns-query.1"
    );
}

