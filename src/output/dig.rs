//! An output format compatible with dig.

use crate::client::Answer;
use domain::base::iana::Rtype;
use domain::base::opt::AllOptData;
use domain::base::ParsedRecord;
use domain::rdata::AllRecordData;
use std::io;

use super::error::OutputError;

//------------ write ---------------------------------------------------------

pub fn write(
    answer: &Answer,
    target: &mut impl io::Write,
) -> Result<(), OutputError> {
    let msg = answer.msg_slice();

    // Header
    let header = msg.header();
    let counts = msg.header_counts();

    writeln!(
        target,
        ";; ->>HEADER<<- opcode: {}, rcode: {}, id: {}",
        header.opcode(),
        header.rcode(),
        header.id()
    )?;
    write!(target, ";; flags: {}", header.flags())?;
    writeln!(
        target,
        "; QUERY: {}, ANSWER: {}, AUTHORITY: {}, ADDITIONAL: {}",
        counts.qdcount(),
        counts.ancount(),
        counts.nscount(),
        counts.arcount()
    )?;

    let opt = msg.opt(); // We need it further down ...

    if let Some(opt) = opt.as_ref() {
        writeln!(target, "\n;; OPT PSEUDOSECTION:")?;
        writeln!(
            target,
            "; EDNS: version {}; flags: {}; udp: {}",
            opt.version(),
            opt.dnssec_ok(),
            opt.udp_payload_size()
        )?;
        for option in opt.opt().iter::<AllOptData<_, _>>() {
            use AllOptData::*;

            match option {
                Ok(opt) => match opt {
                    Nsid(nsid) => writeln!(target, "; NSID: {}", nsid)?,
                    Dau(dau) => writeln!(target, "; DAU: {}", dau)?,
                    Dhu(dhu) => writeln!(target, "; DHU: {}", dhu)?,
                    N3u(n3u) => writeln!(target, "; N3U: {}", n3u)?,
                    Expire(expire) => {
                        writeln!(target, "; EXPIRE: {}", expire)?
                    }
                    TcpKeepalive(opt) => {
                        writeln!(target, "; TCPKEEPALIVE: {}", opt)?
                    }
                    Padding(padding) => {
                        writeln!(target, "; PADDING: {}", padding)?
                    }
                    ClientSubnet(opt) => {
                        writeln!(target, "; CLIENTSUBNET: {}", opt)?
                    }
                    Cookie(cookie) => {
                        writeln!(target, "; COOKIE: {}", cookie)?
                    }
                    Chain(chain) => writeln!(target, "; CHAIN: {}", chain)?,
                    KeyTag(keytag) => {
                        writeln!(target, "; KEYTAG: {}", keytag)?
                    }
                    ExtendedError(extendederror) => {
                        writeln!(target, "; EDE: {}", extendederror)?
                    }
                    Other(other) => {
                        writeln!(target, "; {}", other.code())?;
                    }
                    _ => writeln!(target, "Unknown OPT")?,
                },
                Err(err) => {
                    writeln!(target, "; ERROR: bad option: {}.", err)?;
                }
            }
        }
    }

    // Question
    let questions = msg.question();
    if counts.qdcount() > 0 {
        write!(target, ";; QUESTION SECTION:")?;
        for item in questions {
            let item = item?;
            writeln!(target, "; {}", item)?;
        }
    }

    // Answer
    let section = questions.answer()?;
    if counts.ancount() > 0 {
        writeln!(target, "\n;; ANSWER SECTION:")?;
        for item in section {
            write_record_item(target, &item?)?;
        }

        while answer.has_next() {
            let msg = &mut answer.msg_slice();
            let section = msg.answer().unwrap();
            for item in section {
                write_record_item(target, &item?)?;
            }
        }
    }

    // Authority
    let section = section.next_section()?.unwrap();
    if counts.nscount() > 0 {
        writeln!(target, "\n;; AUTHORITY SECTION:")?;
        for item in section {
            write_record_item(target, &item?)?;
        }
    }

    // Additional
    let section = section.next_section()?.unwrap();
    if counts.arcount() > 1 || (opt.is_none() && counts.arcount() > 0) {
        writeln!(target, "\n;; ADDITIONAL SECTION:")?;
        for item in section {
            let item = item?;
            if item.rtype() != Rtype::OPT {
                write_record_item(target, &item)?;
            }
        }
    }

    // Stats
    let stats = answer.stats();
    writeln!(
        target,
        "\n;; Query time: {} msec",
        stats.duration.num_milliseconds()
    )?;
    writeln!(
        target,
        ";; SERVER: {}#{} ({})",
        stats.server_addr.ip(),
        stats.server_addr.port(),
        stats.server_proto
    )?;
    writeln!(
        target,
        ";; WHEN: {}",
        stats.start.format("%a %b %d %H:%M:%S %Z %Y")
    )?;
    writeln!(target, ";; MSG SIZE  rcvd: {}", msg.as_slice().len())?;

    Ok(())
}

fn write_record_item(
    target: &mut impl io::Write,
    item: &ParsedRecord<&[u8]>,
) -> Result<(), io::Error> {
    let parsed = item.to_any_record::<AllRecordData<_, _>>();

    if parsed.is_err() {
        write!(target, "; ")?;
    }

    let data = match parsed {
        Ok(item) => item.data().to_string(),
        Err(_) => "<invalid data>".into(),
    };

    writeln!(
        target,
        "{}  {}  {}  {}  {}",
        item.owner(),
        item.ttl().as_secs(),
        item.class(),
        item.rtype(),
        data
    )
}
