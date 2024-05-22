//! An output format designed to be read by humans.

use domain::{
    base::{
        iana::Rtype,
        opt::{AllOptData, OptRecord},
        wire::ParseError,
        Header, HeaderCounts, Message, ParsedName, QuestionSection, Record,
    },
    rdata::AllRecordData,
};
use std::io;

use crate::client::Answer;

use super::OutputOptions;

static BOLD: &str = "\x1B[1m";
static UNDERLINE: &str = "\x1B[4m";
static ITALIC: &str = "\x1B[3m";
static RESET: &str = "\x1B[m";

type Rec<'a> = Record<ParsedName<&'a [u8]>, AllRecordData<&'a [u8], ParsedName<&'a [u8]>>>;

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

pub fn write(
    answer: &Answer,
    target: &mut impl io::Write,
    options: &OutputOptions,
) -> io::Result<()> {
    match write_internal(answer, target, options) {
        Ok(()) => Ok(()),
        Err(FormatError::Io(e)) => Err(e),
        Err(FormatError::BadRecord(e)) => {
            writeln!(target, "ERROR: bad record: {e}")?;
            Ok(())
        }
    }
}

fn write_internal(
    answer: &Answer,
    target: &mut impl io::Write,
    options: &OutputOptions,
) -> Result<(), FormatError> {
    let msg = answer.msg_slice();

    let header = msg.header();
    let counts = msg.header_counts();

    // If the long option is not passed, we only show the answer section
    if !options.long {
        let mut answers = msg.question().answer()?.limit_to::<AllRecordData<_, _>>();
        write_answers(target, &mut answers, false)?;
        return Ok(());
    }

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

    let mut section = questions.answer()?.limit_to::<AllRecordData<_, _>>();
    if counts.ancount() > 0 {
        write_answers(target, &mut section, true)?;
    }

    // Authority
    let mut section = section
        .next_section()?
        .unwrap()
        .limit_to::<AllRecordData<_, _>>();
    if counts.nscount() > 0 {
        writeln!(target, "\n{BOLD}AUTHORITY SECTION{RESET}")?;
        write_answer_table(target, &mut section, true)?;
    }

    // Additional
    let section = section
        .next_section()?
        .unwrap()
        .limit_to::<AllRecordData<_, _>>();
    if counts.arcount() > 1 || (opt.is_none() && counts.arcount() > 0) {
        writeln!(target, "\n{BOLD}ADDITIONAL SECTION{RESET}")?;
        write_answer_table(
            target,
            section.filter(|item| item.as_ref().map_or(true, |i| i.rtype() != Rtype::OPT)),
            true,
        )?;
    }

    write_stats(target, msg, answer)?;

    Ok(())
}

fn write_header(
    target: &mut impl io::Write,
    header: Header,
    counts: HeaderCounts,
) -> Result<(), FormatError> {
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
    write_table(target, None, "  ", "  ", &header_rows)?;
    Ok(())
}

fn write_opt(target: &mut impl io::Write, opt: &OptRecord<&[u8]>) -> Result<(), FormatError> {
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

    write_table(target, None, "  ", "  ", &rows)?;
    Ok(())
}

fn write_question(
    target: &mut impl io::Write,
    questions: &QuestionSection<&[u8]>,
) -> Result<(), FormatError> {
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
        .collect::<Result<Vec<_>, FormatError>>()?;

    write_table(
        target,
        Some(["Name", "Type", "Class"]),
        "  ",
        "    ",
        &questions,
    )?;
    Ok(())
}

fn write_answers<'a>(
    target: &mut impl io::Write,
    answers: impl Iterator<Item = Result<Rec<'a>, ParseError>>,
    long: bool,
) -> Result<(), FormatError> {
    if long {
        writeln!(target, "\n{BOLD}ANSWER SECTION{RESET}")?;
    }
    write_answer_table(target, answers, long)
}

fn write_answer_table<'a>(
    target: &mut impl io::Write,
    answers: impl Iterator<Item = Result<Rec<'a>, ParseError>>,
    long: bool,
) -> Result<(), FormatError> {
    let answers = answers
        .map(|a| {
            let a = a?;
            Ok([
                a.owner().to_string(),
                a.ttl().as_secs().to_string(),
                a.class().to_string(),
                a.rtype().to_string(),
                a.data().to_string(),
            ])
        })
        .collect::<Result<Vec<_>, FormatError>>()?;

    write_table(
        target,
        Some(["Owner", "TTL", "Class", "Type", "Data"]),
        if long { "  " } else { "" },
        "    ",
        &answers,
    )?;
    Ok(())
}

fn write_stats(
    target: &mut impl io::Write,
    msg: Message<&[u8]>,
    answer: &Answer,
) -> Result<(), FormatError> {
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
    write_table(target, None, "  ", "  ", &stats)?;
    Ok(())
}

fn write_table<const N: usize>(
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
