//! An output format designed to be read by humans.

use domain::base::iana::Rtype;
use domain::base::opt::{AllOptData, OptRecord};
use domain::base::wire::ParseError;
use domain::base::{Header, HeaderCounts, Message, ParsedRecord, QuestionSection};
use domain::rdata::AllRecordData;
use std::io;

use super::ansi::{BOLD, RESET};
use super::error::OutputError;
use super::ttl;
use crate::client::Answer;

use super::table_writer::TableWriter;

pub fn write(answer: &Answer, target: &mut impl io::Write) -> Result<(), OutputError> {
    let msg = answer.msg_slice();

    let header = msg.header();
    let counts = msg.header_counts();

    write_header(target, header, counts)?;

    // We need opt further down
    let opt = msg.opt();

    if let Some(opt) = &opt {
        write_opt(target, opt)?;
    }

    let questions = msg.question();
    if counts.qdcount() > 0 {
        write_question(target, &questions)?;
    }

    let section = questions.answer()?;
    if counts.ancount() > 0 {
        writeln!(target, "\n{BOLD}ANSWER SECTION{RESET}")?;
        write_answer_table(target, section)?;
    }

    let mut section = section.next_section()?.unwrap();
    if counts.nscount() > 0 {
        writeln!(target, "\n{BOLD}AUTHORITY SECTION{RESET}")?;
        write_answer_table(target, &mut section)?;
    }

    let section = section.next_section()?.unwrap();
    if counts.arcount() > 1 || (opt.is_none() && counts.arcount() > 0) {
        writeln!(target, "\n{BOLD}ADDITIONAL SECTION{RESET}")?;
        write_answer_table(
            target,
            section.filter(|item| item.as_ref().map_or(true, |i| i.rtype() != Rtype::OPT)),
        )?;
    }

    write_stats(target, msg, answer)?;

    Ok(())
}

fn write_header(
    target: &mut impl io::Write,
    header: Header,
    counts: HeaderCounts,
) -> Result<(), OutputError> {
    writeln!(target, "{BOLD}HEADER{RESET}")?;
    let header_rows = [
        ["opcode:".into(), header.opcode().to_string()],
        ["rcode:".into(), header.rcode().to_string()],
        ["id:".into(), header.id().to_string()],
        ["flags:".into(), header.flags().to_string()],
        [
            "records:".into(),
            format!(
                "QUESTION: {}, ANSWER: {}, AUTHORITY: {}, ADDITIONAL: {}",
                counts.qdcount(),
                counts.ancount(),
                counts.nscount(),
                counts.arcount()
            ),
        ],
    ];

    TableWriter {
        indent: "  ",
        rows: &header_rows,
        ..Default::default()
    }
    .write(target)?;

    Ok(())
}

fn write_opt(target: &mut impl io::Write, opt: &OptRecord<&[u8]>) -> Result<(), OutputError> {
    writeln!(target, "\n{BOLD}OPT PSEUDOSECTION{RESET}")?;

    let mut rows = Vec::new();

    rows.push([
        "EDNS".to_string(),
        format!(
            "version: {}; flags: {}; udp: {}",
            opt.version(),
            opt.dnssec_ok(),
            opt.udp_payload_size()
        ),
    ]);

    for option in opt.opt().iter::<AllOptData<_, _>>() {
        use AllOptData::*;

        let (name, value) = match option {
            Ok(opt) => match opt {
                Nsid(nsid) => ("NSID", nsid.to_string()),
                Dau(dau) => ("DAU", dau.to_string()),
                Dhu(dhu) => ("DHU", dhu.to_string()),
                N3u(n3u) => ("N3U", n3u.to_string()),
                Expire(expire) => ("EXPIRE", expire.to_string()),
                TcpKeepalive(opt) => ("TCPKEEPALIVE", opt.to_string()),
                Padding(padding) => ("PADDING", padding.to_string()),
                ClientSubnet(opt) => ("CLIENTSUBNET", opt.to_string()),
                Cookie(cookie) => ("COOKIE: {}", cookie.to_string()),
                Chain(chain) => ("CHAIN", chain.to_string()),
                KeyTag(keytag) => ("KEYTAG", keytag.to_string()),
                ExtendedError(extendederror) => ("EDE", extendederror.to_string()),
                Other(other) => ("OTHER", other.code().to_string()),
                _ => ("ERROR", "Unknown OPT".to_string()),
            },
            Err(err) => ("ERROR", format!("bad option: {}.", err)),
        };

        rows.push([name.to_string(), value]);
    }

    TableWriter {
        indent: "  ",
        rows: &rows,
        ..Default::default()
    }
    .write(target)?;

    Ok(())
}

fn write_question(
    target: &mut impl io::Write,
    questions: &QuestionSection<&[u8]>,
) -> Result<(), OutputError> {
    writeln!(target, "\n{BOLD}QUESTION SECTION{RESET}")?;

    let questions = questions
        .map(|q| {
            let q = q?;
            Ok([
                q.qname().to_string(),
                q.qtype().to_string(),
                q.qclass().to_string(),
            ])
        })
        .collect::<Result<Vec<_>, OutputError>>()?;

    TableWriter {
        indent: "  ",
        spacing: "    ",
        header: Some(["Name", "Type", "Class"]),
        rows: &questions,
        enabled_columns: [true, true, false],
        ..Default::default()
    }
    .write(target)?;
    Ok(())
}

fn write_answer_table<'a>(
    target: &mut impl io::Write,
    answers: impl Iterator<Item = Result<ParsedRecord<'a, &'a [u8]>, ParseError>>,
) -> Result<(), OutputError> {
    let answers = answers
        .map(|item| {
            let item = item?;
            let res = item.to_any_record::<AllRecordData<_, _>>();
            let data = match res {
                Ok(item) => item.data().to_string(),
                Err(_) => "<invalid data>".to_string(),
            };
            Ok([
                item.owner().to_string(),
                ttl::format(item.ttl()),
                item.class().to_string(),
                item.rtype().to_string(),
                data,
            ])
        })
        .collect::<Result<Vec<_>, OutputError>>()?;

    TableWriter {
        indent: "  ",
        spacing: "    ",
        header: Some(["Owner", "TTL", "Class", "Type", "Data"]),
        rows: &answers,
        enabled_columns: [true, true, false, true, true],
        right_aligned: [false, true, false, false, false],
    }
    .write(target)?;
    Ok(())
}

fn write_stats(
    target: &mut impl io::Write,
    msg: Message<&[u8]>,
    answer: &Answer,
) -> Result<(), OutputError> {
    writeln!(target, "\n{BOLD}EXTRA INFO{RESET}")?;
    let stats = answer.stats();
    let stats = [
        [
            "When:".into(),
            stats.start.format("%a %b %d %H:%M:%S %Z %Y").to_string(),
        ],
        [
            "Query time:".into(),
            format!("{} msec", stats.duration.num_milliseconds()),
        ],
        [
            "Server:".into(),
            format!("{}#{}", stats.server_addr.ip(), stats.server_addr.port()),
        ],
        ["Protocol:".into(), stats.server_proto.to_string()],
        [
            "Response size:".into(),
            format!("{} bytes", msg.as_slice().len()),
        ],
    ];

    TableWriter {
        indent: "  ",
        rows: &stats,
        ..Default::default()
    }
    .write(target)?;
    Ok(())
}
