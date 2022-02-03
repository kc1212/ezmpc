use ezmpc::io;

use clap::{App, Arg};
use env_logger;
use ezmpc::io::PrivateConf;
use std::net::SocketAddr;
use std::str::FromStr;

const LISTEN_ADDR_STR: &'static str = "LISTEN_ADDR";
const MAX_TRIPLES_STR: &'static str = "max_triples";
const MAX_RAND_PER_PARTY_STR: &'static str = "max_rand_per_party";

fn main() -> Result<(), ezmpc::error::ApplicationError> {
    env_logger::init();

    #[rustfmt::skip]
    let matches = App::new("ezmpc fake prep")
        .arg(Arg::new(LISTEN_ADDR_STR)
            .help("Set the listening socket address")
            .required(true)
            .index(1))
        .arg(Arg::new(PrivateConf::arg_name())
            .help("Set the private conf files to calculate alpha")
            .setting(clap::ArgSettings::MultipleValues))
        .arg(Arg::new(MAX_RAND_PER_PARTY_STR)
            .help("Set the maximum number of random shares per party")
            .short('r')
            .default_value("100"))
        .arg(Arg::new(MAX_TRIPLES_STR)
            .help("Set the maximum number of triples")
            .short('t')
            .default_value("100"))
        .get_matches();

    let listen_addr: SocketAddr = matches.value_of(LISTEN_ADDR_STR).unwrap().parse()?;

    let fnames: Vec<_> = matches.values_of(PrivateConf::arg_name()).unwrap().collect();
    let mut priv_confs = vec![];
    for fname in fnames {
        let priv_conf = io::PrivateConf::from_file(fname)?;
        priv_confs.push(priv_conf);
    }

    let r = usize::from_str(matches.value_of(MAX_RAND_PER_PARTY_STR).unwrap())?;
    let t = usize::from_str(matches.value_of(MAX_TRIPLES_STR).unwrap())?;

    io::fake_prep_main(listen_addr, priv_confs, r, t)
}
