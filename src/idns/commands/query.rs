//! The query command of _idns._

use std::fmt;
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;
use bytes::Bytes;
use domain::base::iana::{Class, Rtype};
use domain::base::message::Message;
use domain::base::message_builder::MessageBuilder;
use domain::base::name::{Name, ParsedName, ToName, UncertainName};
use domain::base::rdata::RecordData;
use domain::net::client::request::RequestMessage;
use domain::rdata::{AllRecordData, Ns, Soa};
use domain::resolv::stub::StubResolver;
use domain::resolv::stub::conf::ResolvConf;
use crate::idns::client::{Answer, Client, Server, Transport};
use crate::idns::error::Error;
use crate::idns::output::OutputFormat;


//------------ Query ---------------------------------------------------------

#[derive(Clone, Debug, clap::Args)]
pub struct Query {
    /// The name of the resource records to look up
    #[arg(value_name="QUERY_NAME")]
    qname: Name<Vec<u8>>,

    /// The record type to look up
    #[arg(value_name="QUERY_TYPE", default_value = "A")]
    qtype: Rtype,

    /// The server to send the query to. System servers used if missing
    #[arg(short, long, value_name="ADDR_OR_HOST")]
    server: Option<ServerName>,

    /// The port of the server to send query to.
    #[arg(short = 'p', long = "port", requires = "server")]
    port: Option<u16>,

    /// Use only IPv4 for communication.
    #[arg(short = '4', long, conflicts_with = "ipv6")]
    ipv4: bool,

    /// Use only IPv6 for communication.
    #[arg(short = '6', long, conflicts_with = "ipv4")]
    ipv6: bool,

    /// Use only TCP.
    #[arg(short, long)]
    tcp: bool,

    /// Use only UDP.
    #[arg(short, long)]
    udp: bool,

    /// Set the timeout for a query.
    #[arg(long, value_name="SECONDS")]
    timeout: Option<f32>,

    /// Set the number of retries over UDP.
    #[arg(long)]
    retries: Option<u8>,

    /// Set the advertised UDP payload size.
    #[arg(long)]
    udp_payload_size: Option<u16>,

    /// Unset the RD flag in the request.
    #[arg(long)]
    no_rd: bool,

    /// Disable all sanity checks.
    #[arg(long, short = 'f')]
    force: bool,

    /// Verify the answer against an authoritative server.
    #[arg(long)]
    verify: bool,

    /// Select the output format.
    #[arg(long = "format", default_value = "dig")]
    output_format: OutputFormat,
}

/// # Executing the command
///
impl Query {
    pub fn execute(self) -> Result<(), Error> {
        #[allow(clippy::collapsible_if)] // There may be more later ...
        if !self.force {
            if self.qtype == Rtype::AXFR || self.qtype == Rtype::IXFR {
                return Err(
                    "Please use the 'xfr' command for zone transfer.\n\
                     (Use --force to query anyway.)".into()
                );
            }
        }

        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(self.async_execute())
    }

    pub async fn async_execute(self) -> Result<(), Error> {
        let client = match self.server {
            Some(ServerName::Name(ref host)) => self.host_server(host).await?,
            Some(ServerName::Addr(addr)) => self.addr_server(addr),
            None => self.system_server(),
        };

        let answer = client.request(self.create_request()).await?;
        self.output_format.print(&answer)?;
        if self.verify {
            let auth_answer = self.auth_answer().await?;
            if let Some(diff) = Self::diff_answers(
                auth_answer.message(), answer.message()
            )? {
                println!("\n;; Authoritative ANSWER does not match.");
                println!(
                    ";; Difference of ANSWER with authoritative server {}:",
                    auth_answer.stats().server_addr
                );
                self.output_diff(diff);
            }
            else {
                println!("\n;; Authoritative ANSWER matches.");
            }
        }
        Ok(())
    }
}

/// # Configuration
///
impl Query {
    fn timeout(&self) -> Duration {
        Duration::from_secs_f32(self.timeout.unwrap_or(5.))
    }

    fn retries(&self) -> u8 {
        self.retries.unwrap_or(2)
    }

    fn udp_payload_size(&self) -> u16 {
        self.udp_payload_size.unwrap_or(1232)
    }
}


/// # Resolving the server set
///
impl Query {
    /// Resolves a provided server name.
    async fn host_server(
        &self, server: &UncertainName<Vec<u8>>,
    ) -> Result<Client, Error> {
        let resolver = StubResolver::default();
        let answer = match server {
            UncertainName::Absolute(name) => {
                resolver.lookup_host(name).await
            }
            UncertainName::Relative(name) => {
                resolver.search_host(name).await
            }
        }.map_err(|err| err.to_string())?;

        let mut servers = Vec::new();
        for addr in answer.iter() {
            if (addr.is_ipv4() && self.ipv6) || (addr.is_ipv6() && self.ipv4) {
                continue
            }
            servers.push(Server {
                addr: SocketAddr::new(addr, self.port.unwrap_or(53)),
                transport: self.transport(),
                timeout: self.timeout(),
                retries: self.retries.unwrap_or(2),
                udp_payload_size: self.udp_payload_size.unwrap_or(1232),
            });
        }
        Ok(Client::with_servers(servers))
    }

    /// Resolves a provided server name.
    fn addr_server(&self, addr: IpAddr) -> Client {
        Client::with_servers(vec![
            Server {
                addr: SocketAddr::new(addr, self.port.unwrap_or(53)),
                transport: self.transport(),
                timeout: self.timeout(),
                retries: self.retries(),
                udp_payload_size: self.udp_payload_size()
            }
        ])
    }

    /// Creates a client based on the system defaults.
    fn system_server(&self) -> Client {
        let conf = ResolvConf::default();
        Client::with_servers(
            conf.servers.iter().map(|server| {
                Server {
                    addr: server.addr,
                    transport: self.transport(),
                    timeout: server.request_timeout,
                    retries: u8::try_from(conf.options.attempts).unwrap_or(2),
                    udp_payload_size: server.udp_payload_size,
                }
            }).collect()
        )
    }

    fn transport(&self) -> Transport {
        if self.udp {
            Transport::Udp
        }
        else if self.tcp {
            Transport::Tcp
        }
        else {
            Transport::UdpTcp
        }
    }
}

/// # Create the actual query
///
impl Query {
    /// Creates a new request message.
    fn create_request(&self) -> RequestMessage<Vec<u8>> {
        let mut res = MessageBuilder::new_vec();

        res.header_mut().set_rd(!self.no_rd);

        let mut res = res.question();
        res.push((&self.qname, self.qtype)).unwrap();

        RequestMessage::new(res)
    }
}

/// # Get an authoritative answer
impl Query {
    async fn auth_answer(&self) -> Result<Answer, Error> {
        let servers = {
            let resolver = StubResolver::new();
            let apex = self.get_apex(&resolver).await?;
            let ns_set = self.get_ns_set(&apex, &resolver).await?;
            self.get_ns_addrs(&ns_set, &resolver).await?
        };
        Client::with_servers(servers).query((&self.qname, self.qtype)).await
    }

    /// Tries to determine the apex of the zone the requested records live in.
    async fn get_apex(
        &self, resolv: &StubResolver
    ) -> Result<Name<Vec<u8>>, Error> {
        // Ask for the SOA record for the qname.
        let response = resolv.query((&self.qname, Rtype::SOA)).await?;
        
        // The SOA record is in the answer section if the qname is the apex
        // or in the authority section with the apex as the owner name
        // otherwise.
        let mut answer = response.answer()?.limit_to_in::<Soa<_>>();
        if let Some(soa) = answer.next() {
            let soa = soa?;
            if *soa.owner() == self.qname {
                return Ok(self.qname.clone())
            }
            // Strange SOA in the answer section, let’s continue with
            // the authority section.
        }

        let mut authority = answer.next_section()?.unwrap()
            .limit_to_in::<Soa<_>>();
        if let Some(soa) = authority.next() {
            let soa = soa?;
            return Ok(soa.owner().to_name())
        }

        Err("no SOA record".into())
    }

    /// Tries to find the NS set for the given apex name.
    async fn get_ns_set(
        &self, apex: &Name<Vec<u8>>, resolv: &StubResolver
    ) -> Result<Vec<Name<Vec<u8>>>, Error> {
        let response = resolv.query((apex, Rtype::NS)).await?;
        let mut res = Vec::new();
        for record in response.answer()?.limit_to_in::<Ns<_>>() {
            let record = record?;
            if *record.owner() != apex {
                continue;
            }
            res.push(record.data().nsdname().to_name());
        }

        // We could technically get the A and AAAA records from the additional
        // section, but we’re going to ask anyway, so: meh.

        Ok(res)
    }

    /// Tries to get all the addresses for all the name servers.
    async fn get_ns_addrs(
        &self, ns_set: &[Name<Vec<u8>>], resolv: &StubResolver
    ) -> Result<Vec<Server>, Error> {
        let mut res = HashSet::new();
        for ns in ns_set {
            for addr in resolv.lookup_host(ns).await?.iter() {
                res.insert(addr);
            }
        }
        Ok(
            res.into_iter().map(|addr| {
                Server {
                    addr: SocketAddr::new(addr, 53),
                    transport: Transport::UdpTcp,
                    timeout: self.timeout(),
                    retries: self.retries(),
                    udp_payload_size: self.udp_payload_size(),
                }
            }).collect()
        )
    }

    /// Produces a diff between two answer sections.
    ///
    /// Returns `Ok(None)` if the two answer sections are identical apart from
    /// the TTLs.
    #[allow(clippy::mutable_key_type)]
    fn diff_answers(
        left: &Message<Bytes>, right: &Message<Bytes>
    ) -> Result<Option<Vec<DiffItem>>, Error> {
        // Put all the answers into a two hashsets.
        let left = left.answer()?.into_records::<AllRecordData<_, _>>(
        ).filter_map(
            Result::ok
        ).map(|record| {
            let class = record.class();
            let (name, data) = record.into_owner_and_data();
            (name, class, data)
        }).collect::<HashSet<_>>();

        let right = right.answer()?.into_records::<AllRecordData<_, _>>(
        ).filter_map(
            Result::ok
        ).map(|record| {
            let class = record.class();
            let (name, data) = record.into_owner_and_data();
            (name, class, data)
        }).collect::<HashSet<_>>();

        let mut diff = left.intersection(&right).cloned().map(|item| {
            (Action::Unchanged, item)
        }).collect::<Vec<_>>();
        let size = diff.len();

        diff.extend(left.difference(&right).cloned().map(|item| {
            (Action::Removed, item)
        }));

        diff.extend(right.difference(&left).cloned().map(|item| {
            (Action::Added, item)
        }));

        diff.sort_by(|left, right| left.1.cmp(&right.1));

        if size == diff.len() {
            Ok(None)
        }
        else {
            Ok(Some(diff))
        }
    }

    /// Prints the content of a diff.
    fn output_diff(&self, diff: Vec<DiffItem>) {
        for item in diff {
            println!(
                "{}{} {} {} {}",
                item.0, item.1.0, item.1.1, item.1.2.rtype(), item.1.2
            );
        }
    }
}


//------------ ServerName ---------------------------------------------------

#[derive(Clone, Debug)]
enum ServerName {
    Name(UncertainName<Vec<u8>>),
    Addr(IpAddr),
}

impl FromStr for ServerName {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(addr) = IpAddr::from_str(s) {
            Ok(ServerName::Addr(addr))
        }
        else {
            UncertainName::from_str(s).map(Self::Name).map_err(|_|
                "illegal host name"
            )
        }
    }
}


//------------ Action --------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Action {
    Added,
    Removed,
    Unchanged,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(
            match *self {
                Self::Added => "+ ",
                Self::Removed => "- ",
                Self::Unchanged => "  ",
            }
        )
    }
}


//----------- DiffItem -------------------------------------------------------

type DiffItem = (
    Action,
    (
        ParsedName<Bytes>,
        Class,
        AllRecordData<Bytes, ParsedName<Bytes>>
    )
);
