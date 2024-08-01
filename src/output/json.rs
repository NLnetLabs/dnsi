use crate::client::{Answer, Stats};
use bytes::Bytes;
use domain::base::iana::{Class, Opcode};
use domain::base::{ParsedName, Rtype, Ttl};
use domain::rdata::AllRecordData;
use serde::Serialize;
use std::io;

use super::error::OutputError;

#[derive(Serialize)]
struct AnswerOuput {
    message: MessageOutput,
    stats: Stats,
}

#[derive(Serialize)]
struct MessageOutput {
    id: u16,
    qr: bool,
    opcode: Opcode,
    qdcount: u16,
    ancount: u16,
    nscount: u16,
    arcount: u16,
    question: QuestionOutput,
    answer: Vec<RecordOutput>,
    authority: Vec<RecordOutput>,
    additional: Vec<RecordOutput>,
}

#[derive(Serialize)]
struct QuestionOutput {
    name: String,
    r#type: Rtype,
    class: Class,
}

#[derive(Serialize)]
struct RecordOutput {
    owner: String,
    class: Class,
    r#type: Rtype,
    ttl: Ttl,
    data: AllRecordData<Bytes, ParsedName<Bytes>>,
}

pub fn write(
    answer: &Answer,
    target: &mut impl io::Write,
) -> Result<(), OutputError> {
    let msg = answer.message();
    let stats = answer.stats();
    let header = msg.header();
    let counts = msg.header_counts();

    let q = msg.question().next().unwrap().unwrap();

    // We declare them all up front so that we have sensible defaults if the
    // message turns out to be invalid.
    let mut answer = Vec::new();
    let mut authority = Vec::new();
    let mut additional = Vec::new();

    'outer: {
        let Ok(section) = msg.answer() else {
            break 'outer;
        };

        for rec in section.limit_to::<AllRecordData<_, _>>() {
            let Ok(rec) = rec else {
                break;
            };

            answer.push(RecordOutput {
                owner: rec.owner().to_string(),
                class: rec.class(),
                r#type: rec.rtype(),
                ttl: rec.ttl(),
                data: rec.data().clone(),
            });
        }

        let Ok(mut section) = msg.answer() else {
            break 'outer;
        };

        for v in [&mut answer, &mut authority, &mut additional] {
            let iter = section.limit_to::<AllRecordData<_, _>>();

            for rec in iter {
                let Ok(rec) = rec else {
                    break 'outer;
                };

                v.push(RecordOutput {
                    owner: format!("{}.", rec.owner()),
                    class: rec.class(),
                    r#type: rec.rtype(),
                    ttl: rec.ttl(),
                    data: rec.data().clone(),
                });
            }

            let Ok(Some(s)) = section.next_section() else {
                break;
            };
            section = s;
        }
    }

    let output = AnswerOuput {
        message: MessageOutput {
            id: header.id(),
            qr: header.qr(),
            opcode: header.opcode(),
            qdcount: counts.qdcount(),
            ancount: counts.ancount(),
            nscount: counts.nscount(),
            arcount: counts.arcount(),
            question: QuestionOutput {
                name: format!("{}.", q.qname()),
                r#type: q.qtype(),
                class: q.qclass(),
            },
            answer,
            authority,
            additional,
        },
        stats,
    };

    serde_json::to_writer_pretty(target, &output).unwrap();
    Ok(())
}
