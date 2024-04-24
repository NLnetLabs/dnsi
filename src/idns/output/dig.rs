//! An output format compatible with dig.

use std::io;
use domain::base::Message;
use domain::base::iana::Rtype;
use domain::base::opt::AllOptData;
use domain::rdata::AllRecordData;

//------------ write ---------------------------------------------------------

pub fn write(
    msg: Message<&[u8]>, target: &mut impl io::Write
) -> Result<(), io::Error> {
    // Header
    let header = msg.header();
    let counts = msg.header_counts();

    writeln!(target,
        ";; ->>HEADER<<- opcode: {}, rcode: {}, id: {}",
        header.opcode(), header.rcode(), header.id()
    )?;
    write!(target, ";; flags: {}", header.flags())?;
    writeln!(target,
        "; QUERY: {}, ANSWER: {}, AUTHORITY: {}, ADDITIONAL: {}\n",
        counts.qdcount(), counts.ancount(), counts.nscount(), counts.arcount()
    )?;

    let opt = msg.opt(); // We need it further down ...

    if let Some(opt) = opt.as_ref() {
        writeln!(target, "\n;; OPT PSEUDOSECTION:")?;
        writeln!(target,
            "; EDNS: version {}; flags: {}; udp: {}",
            opt.version(), opt.dnssec_ok(), opt.udp_payload_size()
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
                    Chain(chain) => {
                        writeln!(target, "; CHAIN: {}", chain)?
                    }
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
                }
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
            match item {
                Ok(item) => writeln!(target, "; {}", item)?,
                Err(err) => {
                    writeln!(target, "; ERROR: bad question: {}.", err)?;
                    return Ok(())
                }
            }
        }
    }
    
    /* Answer */
    let mut section = match questions.answer() {
        Ok(section) => section.limit_to::<AllRecordData<_, _>>(),
        Err(err) => {
            writeln!(target, "; ERROR: bad question: {}.", err)?;
            return Ok(())
        }
    };
    if counts.ancount() > 0 {
        writeln!(target, "\n;; ANSWER SECTION:")?;
        while let Some(item) = section.next() {
            match item {
                Ok(item) => writeln!(target, "{}", item)?,
                Err(err) => {
                    writeln!(target, "; Error: bad record: {}.", err)?;
                    return Ok(())
                }
            }
        }
    }

    // Authority
    let mut section = match section.next_section() {
        Ok(section) => section.unwrap().limit_to::<AllRecordData<_, _>>(),
        Err(err) => {
            writeln!(target, "; ERROR: bad record: {}.", err)?;
            return Ok(())
        }
    };
    if counts.nscount() > 0 {
        writeln!(target, "\n;; AUTHORITY SECTION:")?;
        while let Some(item) = section.next() {
            match item {
                Ok(item) => writeln!(target, "{}", item)?,
                Err(err) => {
                    writeln!(target, "; Error: bad record: {}.", err)?;
                    return Ok(())
                }
            }
        }
    }

    // Additional
    let section = match section.next_section() {
        Ok(section) => section.unwrap().limit_to::<AllRecordData<_, _>>(),
        Err(err) => {
            writeln!(target, "; ERROR: bad record: {}.", err)?;
            return Ok(())
        }
    };
    if counts.arcount() > 1 || (opt.is_none() && counts.arcount() > 0) {
        writeln!(target, "\n;; ADDITIONAL SECTION:")?;
        for item in section {
            match item {
                Ok(item) => {
                    if item.rtype() != Rtype::OPT {
                        writeln!(target, "{}", item)?
                    }
                }
                Err(err) => {
                    writeln!(target, "; Error: bad record: {}.", err)?;
                    return Ok(())
                }
            }
        }
    }

    Ok(())
}

