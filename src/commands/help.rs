//! The help command of _dnsi._

use crate::error::Error;
use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

//------------ Help ----------------------------------------------------------

#[derive(Clone, Debug, clap::Args)]
pub struct Help {
    /// The command to show the man page for
    #[arg(value_name = "COMMAND")]
    command: Option<String>,
}

impl Help {
    pub fn execute(self) -> Result<(), Error> {
        let page = match self.command.as_deref() {
            None => Self::DNSI_1,
            Some("query") => Self::DNSI_QUERY_1,
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
        let _ = Command::new("man")
            .arg(file.path())
            .status()
            .map_err(|err| format!("Failed to run man: {}", err))?;
        Ok(())
    }
}

impl Help {
    const DNSI_1: &'static [u8] = include_bytes!("../../doc/dnsi.1");
    const DNSI_QUERY_1: &'static [u8] =
        include_bytes!("../../doc/dnsi-query.1");
}
