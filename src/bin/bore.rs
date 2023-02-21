use std::{fmt, io, process};
use std::net::{UdpSocket, IpAddr, SocketAddr};
use clap::{Parser};
use domain::base::{
        Dname, MessageBuilder, Rtype, StaticCompressor, StreamTarget,
        message::Message, opt::AllOptData
};
// use octseq::builder::OctetsBuilder;
use domain::rdata::AllRecordData;
use domain::resolv::stub::conf::ResolvConf;


#[derive(Clone, Debug, Parser)]
#[command(author, version, about, long_about = None)]
#[command(author = "Tom Carpay, NLnet Labs")]
#[command(version = "0.1")]
#[command(about = "A Rusty cousin to drill", long_about = None)]
struct GlobalParamArgs {
    /// The query name that is going to be resolved
    #[arg(value_name="QUERY_NAME")]
    qname: Dname<Vec<u8>>,

    /// The query type of the request. The default is and A record.
    #[arg(long, default_value = "A")]
    qtype: Rtype,

    /// The server that is query is sent to
    #[arg(short = 's', long, value_name="IP_ADDRESS")]
    server: Option<IpAddr>,

    /// The port of the server that is query is sent to
    #[arg(short = 'p', long = "port", value_parser = clap::value_parser!(u16))]
    port: Option<u16>,

    /// Request no recursion on the DNS message. This is true by default.
    #[arg(long = "norecurse")]
    no_rd_bit: bool,

    /// Set the DO bit to request DNSSEC records. The default is false.
    #[arg(long = "do")]
    do_bit: bool,

    /// Request the server NSID. The default is false.
    #[arg(long = "nsid")]
    nsid: bool,

    /// Use only IPv4 for communication. The default is false.
    #[arg(short = '4', long = "do_ipv4")]
    do_ipv4: bool,

    /// Use only IPv4 for communication. The default is false.
    #[arg(short = '6', long = "do_ipv6")]
    do_ipv6: bool,
}

#[derive(Clone, Debug)]
struct Request {
    args: GlobalParamArgs,
    upstream: SocketAddr,
}

impl Request {
    fn configure(args: GlobalParamArgs) -> Result<Self, String> {
        let mut upstreams = ResolvConf::default();

        /* Specify which IP version we use */
        let mut ip_version = 0;
        if args.do_ipv4 && !args.do_ipv6 {
            ip_version = 4;
        }
        else if !args.do_ipv4 && args.do_ipv6 {
            ip_version = 6;
        }
        if args.do_ipv4 && args.do_ipv6 {
            return Err("you cannot specify both -4 and -6".to_string());
        }

        /* Select the default upstream IP if not specified in arguments */
        let upstream: SocketAddr = match (args.server, args.port) {
            (Some(addr), Some(port)) => SocketAddr::new(addr, port),
            (Some(addr), None) => SocketAddr::new(addr, 0),
            (None, Some(port)) => {
                // Select this upstream just to have this var non-empty
                let mut upstream_socketaddr: SocketAddr = upstreams.servers[0].addr;

                for server in &upstreams.servers {
                    if ip_version == 4 && server.addr.is_ipv4() {
                        upstream_socketaddr = server.addr;
                    } else if ip_version == 6 && server.addr.is_ipv6() {
                        upstream_socketaddr = server.addr;
                    } else {
                        return Err("No upstream IP found for specified IP version".to_string());
                    }
                }

                upstreams.servers[0].addr.set_port(port);
                upstream_socketaddr
            },
            (None, None) => upstreams.servers[0].addr,
        };


        Ok(Request {
            args: args.clone(), // @TODO find better way?
            upstream,
        })
    }

    fn process(self) -> Result<(), BoreError> {
        // Bind a UDP socket to a kernel-provided port
        let socket = match self.upstream {
            SocketAddr::V4(_) => UdpSocket::bind("0.0.0.0:0").expect("couldn't bind to address"),
            SocketAddr::V6(_) => UdpSocket::bind("[::]:0").expect("couldn't bind to address"),
        };

        let message = self.create_message()?;

        // Send message off to the server using our socket
        socket.send_to(&message.as_dgram_slice(), self.upstream)?;

        // Create recv buffer
        let mut buffer = vec![0; 1232];

        // Recv in buffer
        socket.recv_from(&mut buffer)?;

        // Parse the response
        let response = Message::from_octets(buffer).map_err(|_| "bad response")?;
        self.print_response(response);

        /* Print message information */
        println!("\n;; SERVER: {}", self.upstream);

        Ok(())
    }


    fn create_message(&self) -> Result<StreamTarget<Vec<u8>>, BoreError> {
        // @TODO create the sections individually to gain more control/flexibility

        // Create a message builder wrapping a compressor wrapping a stream
        // target.
        let mut msg = MessageBuilder::from_target(
            StaticCompressor::new(
                    StreamTarget::new_vec()
            )
        ).unwrap();

        // Set the RD bit and a random ID in the header and proceed to
        // the question section.
        if !self.args.no_rd_bit {
            msg.header_mut().set_rd(true);
        }

        msg.header_mut().set_random_id();
        let mut msg = msg.question();

        // Add a question and proceed to the answer section.
        msg.push((&self.args.qname, self.args.qtype)).unwrap();

        let mut msg = msg.additional();

        // Add an OPT record.
        // @TODO make this configurable
        msg.opt(|opt| {
            opt.set_udp_payload_size(4096);

            if self.args.nsid {
                opt.nsid(b"")?;
            }

            if self.args.do_bit {
                opt.set_dnssec_ok(true);
            }

            Ok(())
        }).unwrap();

        // Convert the builder into the actual message.
        Ok(msg.finish().into_target())
    }

    fn print_response(&self, response: Message<Vec<u8>>) {
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
            if record.as_ref().unwrap().rtype() != Rtype::Opt {
                println!("{}", record.unwrap());
            }
        }

        let opt_record = response.opt().unwrap();

        println!("\n;; EDNS: version {}; flags: {}; udp: {}", // @TODO remove hardcode UDP
            opt_record.version(), opt_record.dnssec_ok(), opt_record.udp_payload_size()); 

        for option in opt_record.iter::<AllOptData<_, _>>() {
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
}


struct BoreError {
    msg: String,
}

impl From<&str> for BoreError {
    fn from(err: &str) -> Self {
        BoreError { msg: err.to_string() }
    }
}

impl From<io::Error> for BoreError {
    fn from(err: io::Error) -> Self {
        BoreError { msg: err.to_string() }
    }
}

impl fmt::Display for BoreError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.msg)
    }
}


fn main() {
    let args = GlobalParamArgs::parse();

    let request = match Request::configure(args) {
        Ok(request) => request,
        Err(err) => {
            println!("Bore configure error: {}", err);
            process::exit(1);
        }
    };

    if let Err(err) = request.process() {
        println!("Bore process error: {}", err);
        process::exit(1);
    }
}

