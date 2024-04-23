//! The query command of _idns._

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;
use bytes::Bytes;
use domain::base::iana::Rtype;
use domain::base::message::Message;
use domain::base::message_builder::MessageBuilder;
use domain::base::name::{Name, UncertainName};
use domain::base::opt::AllOptData;
use domain::net::client;
use domain::net::client::request::{RequestMessage, SendRequest};
use domain::rdata::AllRecordData;
use domain::resolv::StubResolver;
use domain::resolv::stub::conf::ResolvConf;
use crate::idns::error::Error;


#[derive(Clone, Debug, clap::Args)]
pub struct Args {
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
}

impl Args {
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
        self.print_response(answer);
        Ok(())
    }

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

    fn print_response(&self, response: Message<Bytes>) {
        /* Header */
        let header = response.header();

        println!(";; ->>HEADER<<- opcode: {}, rcode: {}, id: {}",
                header.opcode(), header.rcode(), header.id());

        print!(";; flags: {}", header.flags());

        let count = response.header_counts();
        println!(" ; QUERY: {}, ANSWER: {}, AUTHORITY: {}, ADDITIONAL: {}\n",
            count.qdcount(), count.ancount(), count.nscount(), count.arcount());

        /* Question */
        println!(";; QUESTION SECTION:");

        let question_section = response.question();

        for question in question_section {
            println!("; {}", question.unwrap());
        }

        /* Return early if there are no more records */
        if count.ancount() == 0 && count.nscount() == 0 && count.arcount() == 0 {
            println!();
            return;
        }

        /* Answer */
        println!("\n;; ANSWER SECTION:");

        /* Unpack and parse with all known record types */
        let answer_section = response.answer().unwrap().limit_to::<AllRecordData<_, _>>();

        for record in answer_section {
            println!("{}", record.unwrap());
        }

        /* Return early if there are no more records */
        if count.nscount() == 0 && count.arcount() == 0 {
            println!();
            return;
        }

        /* Authority */
        println!("\n;; AUTHORITY SECTION:");

        let authority_section = response.authority().unwrap().limit_to::<AllRecordData<_, _>>();

        for record in authority_section {
            println!("{}", record.unwrap());
        }

        /* Return early if there are no more records */
        if count.arcount() == 0 {
            println!();
            return;
        }

        /* Additional */
        println!("\n;; ADDITIONAL SECTION:");

        let additional_section = response.additional().unwrap().limit_to::<AllRecordData<_, _>>();

        for record in additional_section {
            if record.as_ref().unwrap().rtype() != Rtype::OPT {
                println!("{}", record.unwrap());
            }
        }

        let opt_record = response.opt().unwrap();

        println!("\n;; EDNS: version {}; flags: {}; udp: {}", // @TODO remove hardcode UDP
            opt_record.version(), opt_record.dnssec_ok(), opt_record.udp_payload_size()); 

        for option in opt_record.opt().iter::<AllOptData<_, _>>() {
            let opt = option.unwrap();
            match opt {
                AllOptData::Nsid(nsid) => println!("; NSID: {}", nsid),
                AllOptData::Dau(dau) => println!("; DAU: {}", dau),
                AllOptData::Dhu(dhu) => println!("; DHU: {}", dhu),
                AllOptData::N3u(n3u) => println!("; N3U: {}", n3u),
                AllOptData::Expire(expire) => println!("; EXPIRE: {}", expire),
                AllOptData::TcpKeepalive(tcpkeepalive) => println!("; TCPKEEPALIVE: {}", tcpkeepalive),
                AllOptData::Padding(padding) => println!("; PADDING: {}", padding),
                AllOptData::ClientSubnet(clientsubnet) => println!("; CLIENTSUBNET: {}", clientsubnet),
                AllOptData::Cookie(cookie) => println!("; COOKIE: {}", cookie),
                AllOptData::Chain(chain) => println!("; CHAIN: {}", chain),
                AllOptData::KeyTag(keytag) => println!("; KEYTAG: {}", keytag),
                AllOptData::ExtendedError(extendederror) => println!("; EDE: {}", extendederror),
                _ => println!("NO OPT!"),
            }
        }
    }

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

