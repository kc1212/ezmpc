use ezmpc::io;
use ezmpc::vm;

use clap::{App, Arg};
use env_logger;

const PROG_FILE_STR: &'static str = "PROGRAM";
const INPUT_STR: &'static str = "INPUT";

fn main() -> Result<(), ezmpc::error::ApplicationError> {
    env_logger::init();

    #[rustfmt::skip]
    let matches = App::new("ezmpc online node")
        .arg(Arg::new(io::PublicConf::arg_name())
            .help("Set the public .ron file")
            .takes_value(true)
            .required(true))
        .arg(Arg::new(io::PrivateConf::arg_name())
            .help("Set the private .ron file")
            .takes_value(true)
            .required(true))
        .arg(Arg::new(PROG_FILE_STR)
            .help("Set the program file")
            .required(true)
            .takes_value(true))
        .arg(Arg::new(INPUT_STR)
            .help("Set the secret input to ezmpc")
            .setting(clap::ArgSettings::MultipleValues))
        .get_matches();

    let public_f = matches.value_of(io::PublicConf::arg_name()).unwrap();
    let public_ron = io::PublicConf::from_file(public_f)?;

    let private_f = matches.value_of(io::PrivateConf::arg_name()).unwrap();
    let private_ron = io::PrivateConf::from_file(private_f)?;

    let prog_f = matches.value_of(PROG_FILE_STR).unwrap();
    let prog: Vec<vm::Instruction> = io::read_prog(prog_f)?;

    let inputs: Vec<_> = matches.values_of(INPUT_STR).unwrap().collect();
    let reg = io::create_register(private_ron.id, &prog, inputs)?;
    let res = io::online_node_main(public_ron, private_ron, reg, prog, None)?;

    println!("result: {:?}", res);
    Ok(())
}
