use ezmpc::io;

use clap::{App, Arg};
use env_logger;
use ezmpc::party::Party;

fn main() -> Result<(), ezmpc::error::ApplicationError> {
    env_logger::init();

    #[rustfmt::skip]
    let matches = App::new("ezmpc online node")
        .arg(Arg::new(io::PublicConfig::arg_name())
            .about("Set the public .ron file")
            .required(true)
            .index(1))
        .arg(Arg::new(io::PrivateConfig::arg_name())
            .about("Set the private .ron file")
            .required(true)
            .index(2))
        .get_matches();

    let public_f = matches.value_of(io::PublicConfig::arg_name()).expect("public .ron file is required");
    let public_ron = io::PublicConfig::from_file(public_f)?;

    let private_f = matches.value_of(io::PrivateConfig::arg_name()).expect("private .ron file is required");
    let private_ron = io::PrivateConfig::from_file(private_f)?;

    io::online_node_main(public_ron, private_ron)
}
