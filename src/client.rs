//! The DNS client for _dnsi._

use crate::error::Error;
use bytes::Bytes;
use chrono::{DateTime, Local, TimeDelta};
use domain::base::message::Message;
use domain::base::message_builder::MessageBuilder;
use domain::base::name::ToName;
use domain::base::question::Question;
use domain::net::client::protocol::UdpConnect;
use domain::net::client::request::{
    ComposeRequest, GetResponse, RequestMessage, SendRequest,
};
use domain::net::client::{dgram, stream, tsig, xfr};
use domain::resolv::stub::conf;
use domain::tsig::Key;
use std::fmt;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
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
        tsig_key: Option<Key>,
    ) -> Result<Answer, Error> {
        let mut res = MessageBuilder::new_vec();

        res.header_mut().set_rd(true);
        res.header_mut().set_random_id();

        let mut res = res.question();
        res.push(question.into()).unwrap();

        self.request(RequestMessage::new(res), tsig_key).await
    }

    pub async fn request(
        &self,
        request: RequestMessage<Vec<u8>>,
        tsig_key: Option<Key>,
    ) -> Result<Answer, Error> {
        let mut servers = self.servers.as_slice();
        while let Some((server, tail)) = servers.split_first() {
            match self
                .request_server(request.clone(), tsig_key.clone(), server)
                .await
            {
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

    pub async fn request_server(
        &self,
        request: RequestMessage<Vec<u8>>,
        tsig_key: Option<Key>,
        server: &Server,
    ) -> Result<Answer, Error> {
        match server.transport {
            Transport::Udp => {
                self.request_udp(request, tsig_key, server).await
            }
            Transport::UdpTcp => {
                self.request_udptcp(request, tsig_key, server).await
            }
            Transport::Tcp => {
                self.request_tcp(request, tsig_key, server).await
            }
            Transport::Tls => {
                self.request_tls(request, tsig_key, server).await
            }
        }
    }

    async fn finalize_request(
        mut send_request: Box<dyn GetResponse>,
        mut stats: Stats,
        streaming: bool,
    ) -> Result<Answer, Error> {
        let mut msgs = Vec::with_capacity(1);
        while !send_request.is_stream_complete() {
            msgs.push(send_request.get_response().await?);
            if !streaming {
                break;
            }
        }
        stats.finalize();
        Ok(Answer {
            msgs,
            stats,
            cur_idx: Default::default(),
        })
    }

    pub async fn request_udptcp(
        &self,
        request: RequestMessage<Vec<u8>>,
        tsig_key: Option<Key>,
        server: &Server,
    ) -> Result<Answer, Error> {
        let answer = self
            .request_udp(request.clone(), tsig_key.clone(), server)
            .await?;
        if answer.message().header().tc() {
            self.request_tcp(request, tsig_key, server).await
        } else {
            Ok(answer)
        }
    }

    pub async fn request_udp(
        &self,
        request: RequestMessage<Vec<u8>>,
        tsig_key: Option<Key>,
        server: &Server,
    ) -> Result<Answer, Error> {
        let stats = Stats::new(server.addr, Protocol::Udp);
        let conn = dgram::Connection::with_config(
            UdpConnect::new(server.addr),
            Self::dgram_config(server),
        );
        let conn =
            tsig::Connection::new(tsig_key, xfr::Connection::new(conn));
        let streaming = request.is_streaming();
        let send_request = conn.send_request(request);
        Self::finalize_request(send_request, stats, streaming).await
    }

    pub async fn request_tcp(
        &self,
        request: RequestMessage<Vec<u8>>,
        tsig_key: Option<Key>,
        server: &Server,
    ) -> Result<Answer, Error> {
        let stats = Stats::new(server.addr, Protocol::Tcp);
        let socket = TcpStream::connect(server.addr).await?;
        let (conn, tran) = stream::Connection::with_config(
            socket,
            Self::stream_config(server),
        );
        let conn =
            tsig::Connection::new(tsig_key, xfr::Connection::new(conn));
        tokio::spawn(tran.run());
        let streaming = request.is_streaming();
        let send_request = conn.send_request(request);
        Self::finalize_request(send_request, stats, streaming).await
    }

    pub async fn request_tls(
        &self,
        request: RequestMessage<Vec<u8>>,
        tsig_key: Option<Key>,
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
        let (conn, tran) = stream::Connection::with_config(
            tls_socket,
            Self::stream_config(server),
        );
        let conn =
            tsig::Connection::new(tsig_key, xfr::Connection::new(conn));
        tokio::spawn(tran.run());
        let streaming = request.is_streaming();
        let send_request = conn.send_request(request);
        Self::finalize_request(send_request, stats, streaming).await
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
    msgs: Vec<Message<Bytes>>,
    stats: Stats,
    cur_idx: AtomicUsize,
}

impl Answer {
    pub fn stats(&self) -> Stats {
        self.stats
    }

    pub fn message(&self) -> &Message<Bytes> {
        &self.msgs[self.cur_idx.load(Ordering::SeqCst)]
    }

    pub fn msg_slice(&self) -> Message<&[u8]> {
        self.msgs[self.cur_idx.load(Ordering::SeqCst)].for_slice_ref()
    }

    pub fn has_next(&self) -> bool {
        let old_cur_idx = self.cur_idx.fetch_add(1, Ordering::SeqCst);
        (old_cur_idx + 1) < self.msgs.len()
    }
}

impl AsRef<Message<Bytes>> for Answer {
    fn as_ref(&self) -> &Message<Bytes> {
        &self.msgs[self.cur_idx.load(Ordering::SeqCst)]
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

    fn finalize(&mut self) {
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
