use clap::{App, Arg};
use std::fs::read_to_string;

use ezmpc::io;
use ezmpc::synchronizer;
use std::net::TcpListener;

fn main() -> Result<(), std::io::Error> {
    #[rustfmt::skip]
    let matches = App::new("ezmpc synchronizer")
        .arg(Arg::new("PUBLIC")
            .about("Set the public toml file")
            .required(true)
            .index(1))
        .get_matches();

    let f_name = matches.value_of("PUBLIC").expect("public toml is required");
    let f_content = read_to_string(f_name)?;
    let public_toml: io::PublicConfig = toml::from_str(&f_content)?;

    // TcpListener::bind(public_toml)
    Ok(())
}
