use ezmpc::io;

use clap::{App, Arg};
use env_logger;

fn main() -> Result<(), ezmpc::error::ApplicationError> {
    env_logger::init();

    #[rustfmt::skip]
        let matches = App::new("ezmpc synchronizer")
        .arg(Arg::new(io::PublicConf::arg_name())
            .about("Set the public .ron file")
            .required(true)
            .index(1))
        .arg(Arg::new(io::SynchronizerConfig::arg_name())
            .about("Set the synchronizer .ron file")
            .required(true)
            .index(2))
        .get_matches();

    let public_f = matches.value_of(io::PublicConf::arg_name()).expect("public .ron file is required");
    let public_ron = io::PublicConf::from_file(public_f)?;

    let synchronizer_f = matches
        .value_of(io::SynchronizerConfig::arg_name())
        .expect("synchronizer .ron file is required");
    let synchronizer_ron = io::SynchronizerConfig::from_file(synchronizer_f)?;

    io::synchronizer_main(public_ron, synchronizer_ron)
}
