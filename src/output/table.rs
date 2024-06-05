use std::io;

use domain::{
    base::{wire::ParseError, Rtype},
    rdata::AllRecordData,
};

use super::ttl;
use crate::{client::Answer, output::table_writer::TableWriter};

enum FormatError {
    Io(io::Error),
    BadRecord(ParseError),
}

impl From<io::Error> for FormatError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ParseError> for FormatError {
    fn from(value: ParseError) -> Self {
        Self::BadRecord(value)
    }
}

pub fn write(answer: &Answer, target: &mut impl io::Write) -> io::Result<()> {
    match write_internal(answer, target) {
        Ok(()) => Ok(()),
        Err(FormatError::Io(e)) => Err(e),
        Err(FormatError::BadRecord(e)) => {
            writeln!(target, "ERROR: bad record: {e}")?;
            Ok(())
        }
    }
}

fn write_internal(answer: &Answer, target: &mut impl io::Write) -> Result<(), FormatError> {
    let msg = answer.msg_slice();

    let mut table_rows = Vec::new();

    const SECTION_NAMES: [&str; 3] = ["ANSWER", "AUTHORITY", "ADDITIONAL"];
    let mut section = msg.question().answer()?;

    for name in SECTION_NAMES {
        let mut iter = section
            .limit_to::<AllRecordData<_, _>>()
            .filter(|i| i.as_ref().map_or(true, |i| i.rtype() != Rtype::OPT));

        if let Some(row) = iter.next() {
            let row = row?;
            table_rows.push([
                name.into(),
                row.owner().to_string(),
                ttl::format(row.ttl()),
                row.class().to_string(),
                row.rtype().to_string(),
                row.data().to_string(),
            ]);
        }

        for row in &mut iter {
            let row = row?;
            table_rows.push([
                String::new(),
                row.owner().to_string(),
                ttl::format(row.ttl()),
                row.class().to_string(),
                row.rtype().to_string(),
                row.data().to_string(),
            ]);
        }

        let Some(section2) = section.next_section()? else {
            break;
        };
        section = section2;
    }

    TableWriter {
        spacing: "    ",
        header: Some(["Section", "Owner", "TTL", "Class", "Type", "Data"]),
        rows: &table_rows,
        enabled_columns: [true, true, true, false, true, true],
        right_aligned: [false, false, true, false, false, false],
        ..Default::default()
    }
    .write(target)?;

    Ok(())
}
