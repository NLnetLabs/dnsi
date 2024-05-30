//! Format based on [RFC 8427]
//!
//! [RFC 8427]: https://tools.ietf.org/html/rfc8427

use domain::{
    base::{
        iana::Class,
        name::FlattenInto,
        opt::{AllOptData, OptRecord},
        Header, Name, ParsedName, ParsedRecord, Rtype, UnknownRecordData,
    },
    rdata::AllRecordData,
    utils::base16,
};
use serde_json::{json, Map, Value};

use crate::client::Answer;
use std::io;

pub fn write(answer: &Answer, mut target: impl io::Write) -> Result<(), io::Error> {
    let mut map = serde_json::Map::new();

    fill_map(&mut map, answer);

    serde_json::to_writer_pretty(&mut target, &map).unwrap();
    writeln!(target)
}

fn fill_map(map: &mut Map<String, Value>, answer: &Answer) {
    let stats = answer.stats();
    let msg = answer.msg_slice();

    insert(
        map,
        "dateString",
        stats
            .start
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
    );
    insert(map, "dateSeconds", stats.start.timestamp());
    insert(map, "msgLength", msg.as_slice().len());

    let header = msg.header();
    insert(map, "ID", header.id());
    insert(map, "QR", header.qr() as u8);
    insert(map, "Opcode", header.opcode().to_int());

    insert_many(
        map,
        [
            ("AA", header.aa() as u8),
            ("TC", header.tc() as u8),
            ("RD", header.rd() as u8),
            ("RA", header.ra() as u8),
            ("AD", header.ad() as u8),
            ("CD", header.cd() as u8),
        ],
    );

    insert(map, "RCODE", header.rcode().to_int());

    let counts = msg.header_counts();
    insert_many(
        map,
        [
            ("QDCOUNT", counts.qdcount()),
            ("ANCOUNT", counts.ancount()),
            ("NSCOUNT", counts.nscount()),
            ("ARCOUNT", counts.arcount()),
        ],
    );

    // The RFC says
    //
    // > QNAME - String of the name of the first Question section of the
    // >   message; see Section 2.6 for a description of the contents
    //
    // and the same for the QTYPE and QCLASS so we take the first question
    // (if it's there).
    let mut questions = msg.question();
    if let Some(Ok(q)) = questions.next() {
        insert(map, "QNAME", q.qname().to_string());

        let qtype = q.qtype();
        insert(map, "QTYPE", qtype.to_int());
        if let Some(s) = rtype_mnemomic(qtype) {
            insert(map, "QTYPEname", s);
        }

        let qclass = q.qclass();
        insert(map, "QCLASS", qclass.to_int());
        if let Some(s) = class_mnemomic(qclass) {
            insert(map, "QCLASSname", s);
        }
    }

    // Restart the iterator because we need all records here.
    let questions = msg.question();

    let mut rrs: Vec<Value> = Vec::new();
    for q in questions.flatten() {
        let mut rr = serde_json::Map::new();

        insert(&mut rr, "TYPE", q.qtype().to_int());
        if let Some(s) = rtype_mnemomic(q.qtype()) {
            insert(&mut rr, "TYPEname", s);
        }

        insert(&mut rr, "CLASS", q.qclass().to_int());
        if let Some(s) = class_mnemomic(q.qclass()) {
            insert(&mut rr, "CLASSname", s);
        }

        rrs.push(rr.into());
    }

    insert(map, "questionRRs", rrs);

    let Ok(section) = questions.next_section() else {
        return;
    };

    let mut rrs = Vec::new();
    for a in section.flatten() {
        let mut rr = Map::new();
        record_map(&mut rr, a);
        rrs.push(rr)
    }

    insert(map, "answerRRs", rrs);

    let Ok(Some(section)) = section.next_section() else {
        return;
    };

    let mut rrs = Vec::new();
    for a in section.flatten() {
        let mut rr = Map::new();
        record_map(&mut rr, a);
        rrs.push(rr)
    }

    insert(map, "authorityRRs", rrs);

    let Ok(Some(section)) = section.next_section() else {
        return;
    };

    let mut rrs = Vec::new();
    for a in section.flatten() {
        if a.rtype() == Rtype::OPT {
            continue;
        }
        let mut rr = Map::new();
        record_map(&mut rr, a);
        rrs.push(rr)
    }

    insert(map, "additionalRRs", rrs);

    if let Some(opt) = msg.opt() {
        let mut edns = Map::new();
        edns_map(&mut edns, &opt, header);
        insert(map, "EDNS", edns);
    }
}

fn insert(m: &mut Map<String, Value>, k: impl ToString, v: impl Into<Value>) {
    m.insert(k.to_string(), v.into());
}

fn insert_many<K: ToString, V: Into<Value>>(
    m: &mut Map<String, Value>,
    i: impl IntoIterator<Item = (K, V)>,
) {
    for (k, v) in i {
        m.insert(k.to_string(), v.into());
    }
}

fn record_map(rr: &mut Map<String, Value>, r: ParsedRecord<&[u8]>) {
    let name: Result<Name<Vec<u8>>, _> = r.owner().try_flatten_into();
    if let Ok(name) = name {
        insert(rr, "NAME", name.fmt_with_dot().to_string());
    }

    insert(rr, "TYPE", r.rtype().to_int());
    if let Some(s) = rtype_mnemomic(r.rtype()) {
        insert(rr, "TYPEname", s);
    }

    insert(rr, "CLASS", r.class().to_int());
    if let Some(s) = class_mnemomic(r.class()) {
        insert(rr, "CLASSname", s);
    }

    insert(rr, "TTL", r.ttl().as_secs());

    if let Ok(Some(rec)) = r.to_record::<AllRecordData<&[u8], ParsedName<&[u8]>>>() {
        let ty = rtype_mnemomic(rec.rtype()).unwrap();
        let data = rec.data().to_string();
        insert(rr, format!("rdata{ty}"), data);
    }

    insert(rr, "RDLENGTH", r.rdlen());

    if let Ok(Some(data)) = r.to_record::<UnknownRecordData<&[u8]>>() {
        insert(rr, "RDATAHEX", hex(data.data().data()));
    }
}

/// Based on [draft-peltan-edns-presentation-format-03]
///
/// [draft-peltan-edns-presentation-format-03]: https://www.ietf.org/archive/id/draft-peltan-edns-presentation-format-03.html
fn edns_map(map: &mut Map<String, Value>, opt: &OptRecord<&[u8]>, header: Header) {
    insert(map, "version", opt.version());
    insert(
        map,
        "flags",
        if opt.dnssec_ok() { &["DO"][..] } else { &[] },
    );
    insert(map, "rcode", opt.rcode(header).to_string());
    insert(map, "udpsize", opt.udp_payload_size());
    for option in opt.opt().iter::<AllOptData<_, _>>() {
        use AllOptData::*;
        let Ok(option) = option else {
            continue;
        };
        match option {
            Dau(dau) => insert(
                map,
                "DAU",
                dau.iter().map(|x| x.to_int()).collect::<Vec<_>>(),
            ),
            Dhu(dhu) => insert(
                map,
                "DHU",
                dhu.iter().map(|x| x.to_int()).collect::<Vec<_>>(),
            ),
            N3u(n3u) => insert(
                map,
                "N3U",
                n3u.iter().map(|x| x.to_int()).collect::<Vec<_>>(),
            ),
            Chain(chain) => insert(map, "CHAIN", chain.to_string()),
            Cookie(cookie) => {
                let cc = cookie.client().to_string();
                match cookie.server() {
                    Some(sc) => insert(map, "COOKIE", &[cc, sc.to_string()][..]),
                    None => insert(map, "COOKIE", &[cc][..]),
                };
            }
            Expire(expire) => {
                match expire.expire() {
                    Some(x) => insert(map, "EXPIRE", x),
                    None => insert(map, "EXPIRE", "NONE"),
                };
            }
            ExtendedError(error) => insert(
                map,
                "EDE",
                json!({
                    "CODE": error.code().to_int(),
                    "Purpose": error.code().to_mnemonic().unwrap_or(b""),
                    "TEXT": if let Some(Ok(s)) = error.text() {
                        s
                    } else {
                        ""
                    },
                }),
            ),
            TcpKeepalive(tcpkeepalive) => {
                insert(
                    map,
                    "KEEPALIVE",
                    // According to the EDNS RFC draft, this is not optional,
                    // but it is optional in a DNS messsage.
                    tcpkeepalive.timeout().map(Into::<u16>::into),
                )
            }
            KeyTag(keytag) => insert(map, "KEYTAG", keytag.iter().collect::<Vec<_>>()),
            Nsid(nsid) => insert(
                map,
                "NSID",
                json!({
                    "HEX": hex(nsid.as_slice()),
                    // The draft is inconsistent about TEXT vs TXT
                    "TEXT": std::str::from_utf8(nsid.as_slice()).unwrap_or(""),
                }),
            ),
            Padding(padding) => insert(
                map,
                "PADDING",
                json!({
                    "LENGTH": padding.as_slice().len(),
                    "HEX": hex(padding.as_slice()),
                }),
            ),
            ClientSubnet(subnet) => insert(map, "ECS", subnet.to_string()),
            Other(opt) => insert(
                map,
                format!("OPT{}", opt.code()),
                hex(opt.as_slice())
            ),
            _ => {}
        }
    }
}

fn hex(x: &[u8]) -> String {
    base16::encode_string(x)
}

fn rtype_mnemomic(x: Rtype) -> Option<&'static str> {
    x.to_mnemonic().and_then(|m| std::str::from_utf8(m).ok())
}

fn class_mnemomic(x: Class) -> Option<&'static str> {
    x.to_mnemonic().and_then(|m| std::str::from_utf8(m).ok())
}
