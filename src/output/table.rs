use std::io;

use domain::{
    base::{wire::ParseError, Rtype},
    rdata::AllRecordData,
};

use super::{ITALIC, RESET, UNDERLINE};
use crate::client::Answer;

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

    const SECTION_NAMES: [&'static str; 3] = ["ANSWER", "AUTHORITY", "ADDITIONAL"];
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
                row.ttl().as_secs().to_string(),
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
                row.ttl().as_secs().to_string(),
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

    write_table(
        target,
        Some(["Section", "Owner", "TTL", "Class", "Type", "Data"]),
        "",
        "    ",
        &table_rows,
    )?;

    Ok(())
}

pub fn write_table<const N: usize>(
    target: &mut impl io::Write,
    header: Option<[&'static str; N]>,
    indent: &'static str,
    spacing: &'static str,
    rows: &[[String; N]],
) -> io::Result<()> {
    let mut widths = [0; N];

    if let Some(header) = header {
        for i in 0..N {
            widths[i] = header[i].len();
        }
    }

    for row in rows {
        for i in 0..N {
            widths[i] = widths[i].max(row[i].len());
        }
    }

    if let Some(header) = header {
        write!(target, "{indent}{UNDERLINE}{ITALIC}")?;
        for i in 0..(N - 1) {
            write!(target, "{:<width$}{spacing}", header[i], width = widths[i])?;
        }
        write!(target, "{:<width$}", header[N - 1], width = widths[N - 1])?;
        writeln!(target, "{RESET}")?;
    }

    for row in rows {
        write!(target, "{indent}")?;
        for i in 0..(N - 1) {
            write!(target, "{:<width$}{spacing}", row[i], width = widths[i])?;
        }
        write!(target, "{:<width$}", row[N - 1], width = widths[N - 1])?;
        writeln!(target)?;
    }

    Ok(())
}
