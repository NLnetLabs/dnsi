//! The DNS client for _dnsi._

use crate::error::Error;
use bytes::Bytes;
use chrono::{DateTime, Local, TimeDelta};
use domain::base::message::Message;
use domain::base::message_builder::MessageBuilder;
use domain::base::name::ToName;
use domain::base::question::Question;
use domain::net::client::protocol::UdpConnect;
use domain::net::client::request::{GetResponseMulti, RequestMessage, RequestMessageMulti, SendRequest, SendRequestMulti};
use domain::net::client::{dgram, stream};
use domain::resolv::stub::conf;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};

//------------ Client --------------------------------------------------------

/// The DNS client used by _dnsi._
#[derive(Clone, Debug)]
pub struct Client {
    servers: Vec<Server>,
}

impl Client {
    /// Creates a client using the system configuration.
    pub fn system() -> Self {
        let conf = conf::ResolvConf::default();
        Self {
            servers: conf
                .servers
                .iter()
                .map(|server| Server {
                    addr: server.addr,
                    transport: server.transport.into(),
                    timeout: server.request_timeout,
                    retries: u8::try_from(conf.options.attempts).unwrap_or(2),
                    udp_payload_size: server.udp_payload_size,
                    tls_hostname: None,
                })
                .collect(),
        }
    }

    pub fn with_servers(servers: Vec<Server>) -> Self {
        Self { servers }
    }

    pub async fn query<N: ToName, Q: Into<Question<N>>>(
        &self,
        question: Q,
    ) -> Result<Answer, Error> {
        let mut res = MessageBuilder::new_vec();

        res.header_mut().set_rd(true);
        res.header_mut().set_random_id();

        let mut res = res.question();
        res.push(question.into()).unwrap();

        self.request(RequestMessage::new(res)?).await
    }

    pub async fn request(
        &self,
        request: RequestMessage<Vec<u8>>,
    ) -> Result<Answer, Error> {
        let mut servers = self.servers.as_slice();
        while let Some((server, tail)) = servers.split_first() {
            match self.request_server(request.clone(), server).await {
                Ok(answer) => return Ok(answer),
                Err(err) => {
                    if tail.is_empty() {
                        return Err(err);
                    }
                }
            }
            servers = tail;
        }
        unreachable!()
    }

    pub async fn request_multi(
        &self,
        request: RequestMessageMulti<Vec<u8>>,
    ) -> Result<(Box<dyn GetResponseMulti>, Stats, Box<dyn SendRequestMulti<RequestMessageMulti<Vec<u8>>>>), Error> {
        let mut servers = self.servers.as_slice();
        while let Some((server, tail)) = servers.split_first() {
            match self.request_server_multi(request.clone(), server).await {
                Ok(get_response) => return Ok(get_response),
                Err(err) => {
                    if tail.is_empty() {
                        return Err(err);
                    }
                }
            }
            servers = tail;
        }
        unreachable!()
    }

    pub async fn request_server(
        &self,
        request: RequestMessage<Vec<u8>>,
        server: &Server,
    ) -> Result<Answer, Error> {
        match server.transport {
            Transport::Udp => self.request_udp(request, server).await,
            Transport::UdpTcp => self.request_udptcp(request, server).await,
            Transport::Tcp => self.request_tcp(request, server).await,
            Transport::Tls => self.request_tls(request, server).await,
        }
    }

    pub async fn request_server_multi(
        &self,
        request: RequestMessageMulti<Vec<u8>>,
        server: &Server,
    ) -> Result<(Box<dyn GetResponseMulti>, Stats, Box<dyn SendRequestMulti<RequestMessageMulti<Vec<u8>>>>), Error> {
        match server.transport {
            Transport::Udp => unreachable!(),
            Transport::UdpTcp => unreachable!(),
            Transport::Tcp => self.request_tcp_multi(request, server).await,
            Transport::Tls => self.request_tls_multi(request, server).await,
        }
    }

    pub async fn request_udptcp(
        &self,
        request: RequestMessage<Vec<u8>>,
        server: &Server,
    ) -> Result<Answer, Error> {
        let answer = self.request_udp(request.clone(), server).await?;
        if answer.message.header().tc() {
            self.request_tcp(request, server).await
        } else {
            Ok(answer)
        }
    }

    pub async fn request_udp(
        &self,
        request: RequestMessage<Vec<u8>>,
        server: &Server,
    ) -> Result<Answer, Error> {
        let mut stats = Stats::new(server.addr, Protocol::Udp);
        let conn = dgram::Connection::with_config(
            UdpConnect::new(server.addr),
            Self::dgram_config(server),
        );
        let message = conn.send_request(request).get_response().await?;
        stats.finalize();
        Ok(Answer { message, stats })
    }

    pub async fn request_tcp(
        &self,
        request: RequestMessage<Vec<u8>>,
        server: &Server,
    ) -> Result<Answer, Error> {
        let mut stats = Stats::new(server.addr, Protocol::Tcp);
        let socket = TcpStream::connect(server.addr).await?;
        let (conn, tran) = stream::Connection::<_, RequestMessageMulti<Vec<u8>>>::with_config(
            socket,
            Self::stream_config(server),
        );
        tokio::spawn(tran.run());
        let message = SendRequest::send_request(&conn, request).get_response().await?;
        stats.finalize();
        Ok(Answer { message, stats })
    }

    pub async fn request_tcp_multi(
        &self,
        request: RequestMessageMulti<Vec<u8>>,
        server: &Server,
    ) -> Result<(Box<dyn GetResponseMulti>, Stats, Box<dyn SendRequestMulti<RequestMessageMulti<Vec<u8>>>>), Error> {
        let stats = Stats::new(server.addr, Protocol::Tcp);
        let socket = TcpStream::connect(server.addr).await?;
        let (conn, tran) = stream::Connection::<RequestMessage<Vec<u8>>, _>::with_config(
            socket,
            Self::stream_config(server),
        );
        tokio::spawn(async { tran.run().await; print!("run terminated"); });
        let get_resp = SendRequestMulti::send_request(&conn, request);
        Ok((get_resp, stats, Box::new(conn)))
    }

    pub async fn request_tls(
        &self,
        request: RequestMessage<Vec<u8>>,
        server: &Server,
    ) -> Result<Answer, Error> {
        let root_store = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.into(),
        };
        let client_config = Arc::new(
            ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth(),
        );

        let mut stats = Stats::new(server.addr, Protocol::Tls);
        let tcp_socket = TcpStream::connect(server.addr).await?;
        let tls_connector = tokio_rustls::TlsConnector::from(client_config);
        let server_name = server
            .tls_hostname
            .clone()
            .expect("tls_hostname must be set for tls")
            .try_into()
            .map_err(|_| {
                let s = "Invalid DNS name";
                <&str as Into<Error>>::into(s)
            })?;
        let tls_socket =
            tls_connector.connect(server_name, tcp_socket).await?;
        let (conn, tran) = stream::Connection::<_, RequestMessageMulti<Vec<u8>>>::with_config(
            tls_socket,
            Self::stream_config(server),
        );
        tokio::spawn(tran.run());
        let message = SendRequest::send_request(&conn, request).get_response().await?;
        stats.finalize();
        Ok(Answer { message, stats })
    }

    pub async fn request_tls_multi(
        &self,
        request: RequestMessageMulti<Vec<u8>>,
        server: &Server,
    ) -> Result<(Box<dyn GetResponseMulti>, Stats, Box<dyn SendRequestMulti<RequestMessageMulti<Vec<u8>>>>), Error> {
        let root_store = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.into(),
        };
        let client_config = Arc::new(
            ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth(),
        );

        let stats = Stats::new(server.addr, Protocol::Tls);
        let tcp_socket = TcpStream::connect(server.addr).await?;
        let tls_connector = tokio_rustls::TlsConnector::from(client_config);
        let server_name = server
            .tls_hostname
            .clone()
            .expect("tls_hostname must be set for tls")
            .try_into()
            .map_err(|_| {
                let s = "Invalid DNS name";
                <&str as Into<Error>>::into(s)
            })?;
        let tls_socket =
            tls_connector.connect(server_name, tcp_socket).await?;
        let (conn, tran) = stream::Connection::<RequestMessage<Vec<u8>>, _>::with_config(
            tls_socket,
            Self::stream_config(server),
        );
        tokio::spawn(tran.run());
        let get_resp = SendRequestMulti::send_request(&conn, request);
        Ok((get_resp, stats, Box::new(conn)))
    }

    fn dgram_config(server: &Server) -> dgram::Config {
        let mut res = dgram::Config::new();
        res.set_read_timeout(server.timeout);
        res.set_max_retries(server.retries);
        res.set_udp_payload_size(Some(server.udp_payload_size));
        res
    }

    fn stream_config(server: &Server) -> stream::Config {
        let mut res = stream::Config::new();
        res.set_response_timeout(server.timeout);
        res
    }
}

//------------ Server --------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Server {
    pub addr: SocketAddr,
    pub transport: Transport,
    pub timeout: Duration,
    pub retries: u8,
    pub udp_payload_size: u16,
    pub tls_hostname: Option<String>,
}

//------------ Transport -----------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub enum Transport {
    Udp,
    UdpTcp,
    Tcp,
    Tls,
}

impl From<conf::Transport> for Transport {
    fn from(transport: conf::Transport) -> Self {
        match transport {
            conf::Transport::UdpTcp => Transport::UdpTcp,
            conf::Transport::Tcp => Transport::Tcp,
        }
    }
}

//------------ Answer --------------------------------------------------------

/// An answer for a query.
pub struct Answer {
    message: Message<Bytes>,
    stats: Stats,
}

impl Answer {
    pub fn new(message: Message<Bytes>, stats: Stats) -> Self {
	Answer { message, stats }
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }

    pub fn message(&self) -> &Message<Bytes> {
        &self.message
    }

    pub fn msg_slice(&self) -> Message<&[u8]> {
        self.message.for_slice_ref()
    }
}

impl AsRef<Message<Bytes>> for Answer {
    fn as_ref(&self) -> &Message<Bytes> {
        &self.message
    }
}

//------------ Stats ---------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Stats {
    pub start: DateTime<Local>,
    pub duration: TimeDelta,
    pub server_addr: SocketAddr,
    pub server_proto: Protocol,
}

impl Stats {
    fn new(server_addr: SocketAddr, server_proto: Protocol) -> Self {
        Stats {
            start: Local::now(),
            duration: Default::default(),
            server_addr,
            server_proto,
        }
    }

    pub fn finalize(&mut self) {
        self.duration = Local::now() - self.start;
    }
}

//------------ Protocol ------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub enum Protocol {
    Udp,
    Tcp,
    Tls,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match *self {
            Protocol::Udp => "UDP",
            Protocol::Tcp => "TCP",
            Protocol::Tls => "TLS",
        })
    }
}
