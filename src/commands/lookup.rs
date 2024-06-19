//! The lookup command of _dnsi._

use crate::error::Error;
use domain::base::name::UncertainName;
use domain::resolv::stub::StubResolver;
use std::net::IpAddr;
use std::str::FromStr;

//------------ Lookup --------------------------------------------------------

#[derive(Clone, Debug, clap::Args)]
pub struct Lookup {
    /// The host or address to look up.
    #[arg(value_name = "HOST_OR_ADDR")]
    names: Vec<ServerName>,
}

/// # Executing the command
///
impl Lookup {
    pub fn execute(self) -> Result<(), Error> {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(self.async_execute())
    }

    pub async fn async_execute(self) -> Result<(), Error> {
        let resolver = StubResolver::new();

        let mut res = Ok(());
        let mut names = self.names.iter();

        if let Some(name) = names.next() {
            res = res.and(self.execute_one_name(&resolver, name).await);
        }

        for name in names {
            println!();
            res = res.and(self.execute_one_name(&resolver, name).await);
        }

        res.map_err(|_| "not all lookups have succeeded".into())
    }

    async fn execute_one_name(
        &self,
        resolver: &StubResolver,
        name: &ServerName,
    ) -> Result<(), ()> {
        let res = match name {
            ServerName::Name(host) => forward(resolver, host).await,
            ServerName::Addr(addr) => reverse(resolver, *addr).await,
        };

        if let Err(err) = res {
            eprintln!("{err}");
            return Err(());
        }

        Ok(())
    }
}

async fn forward(
    resolver: &StubResolver,
    name: &UncertainName<Vec<u8>>,
) -> Result<(), Error> {
    let answer = match name {
        UncertainName::Absolute(ref name) => {
            resolver.lookup_host(name).await?
        }
        UncertainName::Relative(ref name) => {
            resolver.search_host(name).await?
        }
    };

    print!("{name}");

    let canon = answer.canonical_name();
    if canon != answer.qname() {
        print!(" (alias for {canon})");
    }

    println!();

    let addrs: Vec<_> = answer.iter().collect();
    if addrs.is_empty() {
        println!("  <no addresses found>");
    } else {
        for addr in addrs {
            println!("  {addr}");
        }
    }

    Ok(())
}

async fn reverse(resolver: &StubResolver, addr: IpAddr) -> Result<(), Error> {
    let answer = resolver.lookup_addr(addr).await?;
    println!("{addr}");

    let hosts: Vec<_> = answer.iter().collect();
    if hosts.is_empty() {
        println!("  <no hosts found>");
    } else {
        for name in hosts {
            println!("  {name}");
        }
    }

    Ok(())
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
                .map_err(|_| "illegal host name or address")
        }
    }
}
