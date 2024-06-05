use std::io;

use domain::base::Rtype;
use domain::rdata::AllRecordData;

use super::{error::OutputError, ttl};
use crate::{client::Answer, output::table_writer::TableWriter};

pub fn write(answer: &Answer, target: &mut impl io::Write) -> Result<(), OutputError> {
    let msg = answer.msg_slice();

    let mut table_rows = Vec::new();

    const SECTION_NAMES: [&str; 3] = ["ANSWER", "AUTHORITY", "ADDITIONAL"];
    let mut section = msg.question().answer()?;

    for name in SECTION_NAMES {
        let mut iter = section.filter(|i| i.as_ref().map_or(true, |i| i.rtype() != Rtype::OPT));

        // The first row of each section gets the section name
        if let Some(row) = iter.next() {
            let row = row?;
            let data = match row.to_any_record::<AllRecordData<_, _>>() {
                Ok(row) => row.data().to_string(),
                Err(_) => "<invalid data>".into(),
            };
            table_rows.push([
                name.into(),
                row.owner().to_string(),
                ttl::format(row.ttl()),
                row.class().to_string(),
                row.rtype().to_string(),
                data,
            ]);
        }

        // The rest of the rows we show without section name
        for row in &mut iter {
            let row = row?;
            let data = match row.to_any_record::<AllRecordData<_, _>>() {
                Ok(row) => row.data().to_string(),
                Err(_) => "<invalid data>".into(),
            };
            table_rows.push([
                String::new(),
                row.owner().to_string(),
                ttl::format(row.ttl()),
                row.class().to_string(),
                row.rtype().to_string(),
                data,
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
