//! The query command of _idns._

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;
use bytes::Bytes;
use domain::base::iana::Rtype;
use domain::base::message::Message;
use domain::base::message_builder::MessageBuilder;
use domain::base::name::{Name, ToName, UncertainName};
use domain::net::client;
use domain::net::client::request::{RequestMessage, SendRequest};
use domain::rdata::{AllRecordData, Ns, Soa};
use domain::resolv::StubResolver;
use domain::resolv::stub;
use domain::resolv::stub::conf::{ResolvConf, ServerConf, Transport};
use crate::idns::error::Error;
use crate::idns::output::OutputFormat;


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

    /// Use the given message ID. Random if missing.
    #[arg(long)]
    id: Option<u16>,

    /// Unset the RD flag in the request.
    #[arg(long)]
    no_rd: bool,

    /// Disable all sanity checks.
    #[arg(long, short = 'f')]
    force: bool,

    /// Set the timeout for a query.
    #[arg(long, value_name="SECONDS")]
    timeout: Option<f32>,

    /// Set the number of retries over UDP.
    #[arg(long)]
    retries: Option<u8>,

    /// Set the advertised UDP payload size.
    #[arg(long)]
    udp_payload_size: Option<u16>,

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
        let servers = match self.server {
            Some(ServerName::Name(ref host)) => self.host_server(host).await?,
            Some(ServerName::Addr(addr)) => self.addr_server(addr),
            None => self.default_server(),
        };

        let request = self.create_request();
        let answer = self.send_and_receive(servers, request).await?;
        self.output_format.print(answer.for_slice_ref())?;
        if self.verify {
            let auth_answer = self.auth_answer().await?;
            if Self::eq_answer(
                answer.for_slice_ref(), auth_answer.for_slice_ref()
            ) {
                println!("\n;; Authoritative answer matches.");
            }
            else {
                println!("\n;; Authoritative answer does not match.");
                println!(";; AUTHORITATIVE ANSWER");
                self.output_format.print(auth_answer.for_slice_ref())?;
            }
        }
        Ok(())
    }
}


/// # Resolving the server set
///
impl Query {
    /// Resolves a provided server name.
    async fn host_server(
        &self, server: &UncertainName<Vec<u8>>
    ) -> Result<Vec<SocketAddr>, Error> {
        let resolver = StubResolver::new();
        let answer = match server {
            UncertainName::Absolute(name) => {
                resolver.lookup_host(name).await
            }
            UncertainName::Relative(name) => {
                resolver.search_host(name).await
            }
        }.map_err(|err| err.to_string())?;

        let mut res = Vec::new();
        for addr in answer.iter() {
            if (addr.is_ipv4() && self.ipv6) || (addr.is_ipv6() && self.ipv4) {
                continue
            }
            res.push(SocketAddr::new(addr, self.port.unwrap_or(53)));
        }
        Ok(res)
    }

    /// Resolves a provided server name.
    fn addr_server(&self, addr: IpAddr) -> Vec<SocketAddr> {
        vec![SocketAddr::new(addr, self.port.unwrap_or(53))]
    }

    /// Create the default server configuration.
    fn default_server(&self) -> Vec<SocketAddr> {
        let mut res = HashSet::new();
        for server in ResolvConf::default().servers {
            res.insert(server.addr);
        }
        res.into_iter().collect()
    }
}

/// # Handling the actual query
///
impl Query {
    /// Creates a new request message.
    fn create_request(&self) -> RequestMessage<Vec<u8>> {
        let mut res = MessageBuilder::new_vec();

        res.header_mut().set_rd(!self.no_rd);
        if let Some(id) = self.id {
            res.header_mut().set_id(id)
        }
        else {
            res.header_mut().set_random_id();
        }

        let mut res = res.question();
        res.push((&self.qname, self.qtype)).unwrap();

        RequestMessage::new(res)
    }

    /// Sends the request and returns a response.
    async fn send_and_receive(
        &self,
        mut server: Vec<SocketAddr>,
        request: RequestMessage<Vec<u8>>
    ) -> Result<Message<Bytes>, Error> {
        while let Some(addr) = server.pop() {
            match self.send_and_receive_single(addr, request.clone()).await {
                Ok(answer) => return Ok(answer),
                Err(err) => {
                    if server.is_empty() {
                        return Err(err)
                    }
                }
            }
        }
        unreachable!()
    }

    /// Sends the request to exactly one server and returns the response.
    async fn send_and_receive_single(
        &self,
        server: SocketAddr,
        request: RequestMessage<Vec<u8>>
    ) -> Result<Message<Bytes>, Error> {
        let tcp_connect = client::protocol::TcpConnect::new(server);
        if self.tcp {
            let (conn, tran) = client::multi_stream::Connection::with_config(
                tcp_connect,
                self.multi_stream_config(),
            );
            tokio::spawn(tran.run());
            conn.send_request(request).get_response().await.map_err(|err| {
                err.to_string().into()
            })
        }
        else {
            let udp_connect = client::protocol::UdpConnect::new(server);
            let (conn, tran) = client::dgram_stream::Connection::with_config(
                udp_connect, tcp_connect, self.dgram_stream_config(),
            );
            tokio::spawn(tran.run());
            conn.send_request(request).get_response().await.map_err(|err| {
                err.to_string().into()
            })
        }
    }
}

/// # Server configurations
///
impl Query {
    fn timeout(&self) -> Option<Duration> {
        self.timeout.map(Duration::from_secs_f32)
    }

    fn dgram_config(&self) -> client::dgram::Config {
        let mut res = client::dgram::Config::new();
        if let Some(timeout) = self.timeout() {
            res.set_read_timeout(timeout);
        }
        if let Some(retries) = self.retries {
            res.set_max_retries(retries)
        }
        if let Some(size) = self.udp_payload_size {
            res.set_udp_payload_size(Some(size))
        }
        res
    }

    fn stream_config(&self) -> client::stream::Config {
        let mut res = client::stream::Config::new();
        if let Some(timeout) = self.timeout() {
            res.set_response_timeout(timeout);
        }
        res
    }

    fn multi_stream_config(&self) -> client::multi_stream::Config {
        client::multi_stream::Config::from(self.stream_config())
    }

    fn dgram_stream_config(&self) -> client::dgram_stream::Config {
        client::dgram_stream::Config::from_parts(
            self.dgram_config(), self.multi_stream_config()
        )
    }
}

/// # Get an authoritative answer
impl Query {
    async fn auth_answer(&self) -> Result<stub::Answer, Error> {
        let addrs = {
            let resolver = StubResolver::new();
            let apex = self.get_apex(&resolver).await?;
            let ns_set = self.get_ns_set(&apex, &resolver).await?;
            self.get_ns_addrs(&ns_set, &resolver).await?
        };

        let resolver = StubResolver::from_conf(
            ResolvConf {
                servers: addrs.into_iter().map(|addr| {
                    ServerConf::new(
                        SocketAddr::new(addr, 53), Transport::UdpTcp
                    )
                }).collect(),
                options: Default::default(),
            }
        );
        
        resolver.query((&self.qname, self.qtype)).await.map_err(Into::into)
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
        while let Some(soa) = answer.next() {
            let soa = soa?;
            if *soa.owner() == self.qname {
                return Ok(self.qname.clone())
            }
            else {
                // Strange SOA in the answer section, let’s continue with
                // the authority section.
                break;
            }
        }

        let mut authority = answer.next_section()?.unwrap()
            .limit_to_in::<Soa<_>>();
        while let Some(soa) = authority.next() {
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
    ) -> Result<Vec<IpAddr>, Error> {
        let mut res = HashSet::new();
        for ns in ns_set {
            for addr in resolv.lookup_host(ns).await?.iter() {
                res.insert(addr);
            }
        }
        Ok(res.into_iter().collect())
    }

    /// Compares the answer section of two messages.
    fn eq_answer(left: Message<&[u8]>, right: Message<&[u8]>) -> bool {
        if left.header_counts().ancount() != right.header_counts().ancount() {
            return false
        }
        let left = match left.answer() {
            Ok(answer) => {
                answer.into_records::<AllRecordData<_, _>>().map(|record| {
                    match record {
                        Ok(record) => {
                            let class = record.class();
                            let (name, data) = record.into_owner_and_data();
                            Some((name, class, data))
                        }
                        Err(_) => None,
                    }
                }).collect::<HashSet<_>>()
            }
            Err(_) => return false,
        };
        let right = match right.answer() {
            Ok(answer) => {
                answer.into_records::<AllRecordData<_, _>>().map(|record| {
                    match record {
                        Ok(record) => {
                            let class = record.class();
                            let (name, data) = record.into_owner_and_data();
                            Some((name, class, data))
                        }
                        Err(_) => None,
                    }
                }).collect::<HashSet<_>>()
            }
            Err(_) => return false,
        };
        left == right
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

