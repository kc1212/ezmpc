use ezmpc::io;
use ezmpc::message::{SyncMsg, SyncReplyMsg};
use ezmpc::synchronizer::Synchronizer;

use clap::{App, Arg};
use env_logger;
use log::{error, info};
use ron;
use std::fs::read_to_string;
use std::net::{SocketAddr, TcpListener};

const PUBLIC_CONFIG: &str = "PUBLIC_CONFIG";
const SYNCHRONIZER_CONFIG: &str = "SYNCHRONIZER_CONFIG";

fn main() -> Result<(), ezmpc::error::ApplicationError> {
    env_logger::init();

    #[rustfmt::skip]
    let matches = App::new("ezmpc synchronizer")
        .arg(Arg::new(PUBLIC_CONFIG)
            .about("Set the public .ron file")
            .required(true)
            .index(1))
        .arg(Arg::new(SYNCHRONIZER_CONFIG)
            .about("Set the synchronizer .ron file")
            .required(true)
            .index(2))
        .get_matches();

    let public_f = matches.value_of(PUBLIC_CONFIG).expect("public .ron file is required");
    let public_str = read_to_string(public_f)?;
    let public_ron: io::PublicConfig = ron::from_str(&public_str)?;

    let synchronizer_f = matches.value_of(SYNCHRONIZER_CONFIG).expect("synchronizer .ron file is required");
    let synchronizer_str = read_to_string(synchronizer_f)?;
    let synchronizer_ron: io::SynchronizerConfig = ron::from_str(&synchronizer_str)?;

    let listener = TcpListener::bind(synchronizer_ron.listen_addr)?;
    info!("[{:?}] I am listening", listener.local_addr());

    let mut peer_handlers = vec![];
    let mut connected_peers = vec![];
    let mut peer_sender_chans = vec![];
    let mut peer_receiver_chans = vec![];
    let mut peer_shutdown_chans = vec![];

    let peers: Vec<SocketAddr> = public_ron.nodes.iter().map(|x| x.addr).collect();
    for stream_result in listener.incoming() {
        let stream = stream_result?;
        if connected_peers.len() == peers.len() {
            info!("[{:?}] all peers connected", stream.local_addr());
            break;
        }
        match stream.peer_addr() {
            Ok(addr) => {
                if peers.contains(&addr) && !connected_peers.contains(&addr) {
                    info!("[{:?}] found peer {:?}", stream.local_addr(), &addr);
                    let (s, r, shutdown_s, h) = io::wrap_tcpstream::<SyncMsg, SyncReplyMsg>(stream);
                    peer_sender_chans.push(s);
                    peer_receiver_chans.push(r);
                    peer_shutdown_chans.push(shutdown_s);
                    peer_handlers.push(h);
                    connected_peers.push(addr);
                }
            }
            Err(e) => {
                error!("[{:?}] unable to get peer address: {:?}", stream.local_addr(), e);
                continue;
            }
        }
    }

    let sync_handle = Synchronizer::spawn(peer_sender_chans, peer_receiver_chans);
    sync_handle.join().expect("synchronizer thread panicked")?;
    for h in peer_handlers {
        h.join().unwrap();
    }
    Ok(())
}
