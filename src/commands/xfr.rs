//! The xfr command of _dnsi._

use crate::client::{Answer, Client, Server, Transport};
use crate::error::Error;
use crate::output::OutputOptions;
use crate::Args;
use clap::error::ErrorKind;
use clap::CommandFactory;
use domain::base::iana::Rtype;
use domain::base::message_builder::MessageBuilder;
use domain::base::name::{Name, UncertainName};
use domain::base::Serial;
use domain::base::Ttl;
use domain::net::client::request::{
    GetResponseMulti, RequestMessage, RequestMessageMulti,
};
use domain::rdata::Soa;
use domain::resolv::stub::conf::ResolvConf;
use domain::resolv::stub::StubResolver;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

//------------ Xfr -----------------------------------------------------------

#[derive(Clone, Debug, clap::Args)]
pub struct Xfr {
    /// The name of the resource records to look up
    #[arg(value_name = "QUERY_NAME_OR_ADDR")]
    qname: NameOrAddr,

    /// Set the IXFR Serial
    #[arg(long = "ixfr")]
    ixfr: Option<Serial>,

    /// The server to send the query to. System servers used if missing
    #[arg(short, long, value_name = "ADDR_OR_HOST")]
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

    /// Try UDP first with fallback to TCP, otherwise use only TCP.
    ///
    /// Only permitted with IXFR as UDP is not permitted for AXFR.
    #[arg(short, long)]
    udp: bool,

    /// Use TLS.
    #[arg(long)]
    tls: bool,

    /// The name of the server for SNI and certificate verification.
    #[arg(long = "tls-hostname")]
    tls_hostname: Option<String>,

    /// Set the timeout for a query.
    #[arg(long, value_name = "SECONDS")]
    timeout: Option<f32>,

    /// Disable all sanity checks.
    #[arg(long, short = 'f')]
    force: bool,

    /// Output options.
    #[command(flatten)]
    output: OutputOptions,
}

/// # Executing the command
///
impl Xfr {
    pub fn execute(self) -> Result<(), Error> {
        // Per RFC 5936 section 4.2 "AXFR sessions over UDP transport are not
        // defined".
        //
        // RFC 1995 section 2 says "a client should first make an IXFR query
        // using UDP" but as RFC 9103 section 5.2 states "it is noted that
        // most of the widely used open-source implementations of
        // authoritative name servers (including both [BIND] and [NSD]) do
        // IXFR using TCP by default in their latest releases" and thus we
        // default to TCP for IXFR, using UDP first must be requested
        // explicity.
        if self.udp && self.ixfr.is_none() {
            // Based on https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html#custom-validation.
            let mut cmd = Args::command();
            cmd.error(
                ErrorKind::ArgumentConflict,
                "UDP is only permitted with IXFR",
            )
            .exit();
        }

        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(self.async_execute())
    }

    pub async fn async_execute(mut self) -> Result<(), Error> {
        let client = match self.server {
            Some(ServerName::Name(ref host)) => {
                if self.tls_hostname.is_none() {
                    self.tls_hostname = Some(host.to_string());
                }
                self.host_server(host).await?
            }
            Some(ServerName::Addr(addr)) => {
                if self.tls && self.tls_hostname.is_none() {
                    return Err(
                        "--tls-hostname is required for TLS transport".into(),
                    );
                }
                self.addr_server(addr)
            }
            None => {
                if self.tls {
                    return Err(
                        "--server is required for TLS transport".into()
                    );
                }
                self.system_server()
            }
        };

        match self.transport() {
            Transport::Udp | Transport::UdpTcp => {
                let ans = client.request(self.create_request()?).await?;
                self.output.format.print(&ans)?;
            }
            Transport::Tcp | Transport::Tls => {
                let (mut get_resp, mut stats, _conn) = client
                    .request_multi(self.create_multi_request()?)
                    .await?;
                loop {
                    let resp =
                        GetResponseMulti::get_response(get_resp.as_mut())
                            .await;
                    stats.finalize();
                    let resp = resp?;
                    let resp = match resp {
                        Some(resp) => resp,
                        None => break,
                    };
                    let ans = Answer::new(resp, stats);
                    self.output.format.print(&ans)?;
                }
            }
        }

        Ok(())
    }
}

/// # Configuration
///
impl Xfr {
    fn timeout(&self) -> Duration {
        Duration::from_secs_f32(self.timeout.unwrap_or(5.))
    }

    fn retries(&self) -> u8 {
        2
    }

    fn udp_payload_size(&self) -> u16 {
        1232
    }
}

/// # Resolving the server set
///
impl Xfr {
    /// Resolves a provided server name.
    async fn host_server(
        &self,
        server: &UncertainName<Vec<u8>>,
    ) -> Result<Client, Error> {
        let resolver = StubResolver::default();
        let answer = match server {
            UncertainName::Absolute(name) => resolver.lookup_host(name).await,
            UncertainName::Relative(name) => resolver.search_host(name).await,
        }
        .map_err(|err| err.to_string())?;

        let mut servers = Vec::new();
        for addr in answer.iter() {
            if (addr.is_ipv4() && self.ipv6) || (addr.is_ipv6() && self.ipv4)
            {
                continue;
            }
            servers.push(Server {
                addr: SocketAddr::new(
                    addr,
                    self.port.unwrap_or({
                        if self.tls {
                            853
                        } else {
                            53
                        }
                    }),
                ),
                transport: self.transport(),
                timeout: self.timeout(),
                retries: 2,
                udp_payload_size: 1232,
                tls_hostname: self.tls_hostname.clone(),
            });
        }
        Ok(Client::with_servers(servers))
    }

    /// Resolves a provided server name.
    fn addr_server(&self, addr: IpAddr) -> Client {
        Client::with_servers(vec![Server {
            addr: SocketAddr::new(
                addr,
                self.port.unwrap_or(if self.tls { 853 } else { 53 }),
            ),
            transport: self.transport(),
            timeout: self.timeout(),
            retries: self.retries(),
            udp_payload_size: self.udp_payload_size(),
            tls_hostname: self.tls_hostname.clone(),
        }])
    }

    /// Creates a client based on the system defaults.
    fn system_server(&self) -> Client {
        let conf = ResolvConf::default();
        Client::with_servers(
            conf.servers
                .iter()
                .map(|server| Server {
                    addr: server.addr,
                    transport: self.transport(),
                    timeout: server.request_timeout,
                    retries: u8::try_from(conf.options.attempts).unwrap_or(2),
                    udp_payload_size: server.udp_payload_size,
                    tls_hostname: None,
                })
                .collect(),
        )
    }

    fn transport(&self) -> Transport {
        if self.tls {
            Transport::Tls
        } else if self.udp {
            Transport::UdpTcp
        } else {
            Transport::Tcp
        }
    }
}

/// # Create the actual query
///
impl Xfr {
    /// Creates a new request message.
    fn create_request(&self) -> Result<RequestMessage<Vec<u8>>, Error> {
        Ok(RequestMessage::new(self.create_message())?)
    }

    /// Creates a new request message.
    fn create_multi_request(
        &self,
    ) -> Result<RequestMessageMulti<Vec<u8>>, Error> {
        Ok(RequestMessageMulti::new(self.create_message())?)
    }

    fn create_message(
        &self,
    ) -> domain::base::message_builder::AdditionalBuilder<Vec<u8>> {
        let res = MessageBuilder::new_vec();

        let mut res = res.question();
        let add = match self.ixfr {
            None => {
                res.push((&self.qname.to_name(), Rtype::AXFR)).unwrap();
                res.additional()
            }
            Some(serial) => {
                res.push((&self.qname.to_name(), Rtype::IXFR)).unwrap();
                let mut auth = res.authority();
                let soa = Soa::new(
                    Name::root_ref(),
                    Name::root_ref(),
                    serial,
                    Ttl::ZERO,
                    Ttl::ZERO,
                    Ttl::ZERO,
                    Ttl::ZERO,
                );
                auth.push((&self.qname.to_name(), 0, soa)).unwrap();
                auth.additional()
            }
        };
        add
    }
}

/// # Get an authoritative answer
impl Xfr {
    /*
    fn qtype(&self) -> Rtype {
        match self.qtype {
            Some(qtype) => qtype,
            None => match self.qname {
                NameOrAddr::Addr(_) => Rtype::PTR,
                NameOrAddr::Name(_) => Rtype::AAAA,
            },
        }
    }
    */
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
        } else {
            UncertainName::from_str(s)
                .map(Self::Name)
                .map_err(|_| "illegal host name")
        }
    }
}

//------------ NameOrAddr ----------------------------------------------------

#[derive(Clone, Debug)]
enum NameOrAddr {
    Name(Name<Vec<u8>>),
    Addr(IpAddr),
}

impl NameOrAddr {
    fn to_name(&self) -> Name<Vec<u8>> {
        match &self {
            NameOrAddr::Name(host) => host.clone(),
            NameOrAddr::Addr(addr) => {
                Name::<Vec<u8>>::reverse_from_addr(*addr).unwrap()
            }
        }
    }
}

impl FromStr for NameOrAddr {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(addr) = IpAddr::from_str(s) {
            Ok(NameOrAddr::Addr(addr))
        } else {
            Name::from_str(s)
                .map(Self::Name)
                .map_err(|_| "illegal host name")
        }
    }
}
