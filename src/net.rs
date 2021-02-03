use bincode;
use crossbeam::channel::{bounded, select, Receiver, Sender};
use log::{error, info};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::thread::JoinHandle;
use std::io;

use crate::message::*;
use crate::algebra::Fp;
use crate::error::ApplicationError;
use crate::message::PartyID;
use crate::{vm, party};

const TCPSTREAM_CAP: usize = 1000;
const LENGTH_BYTES: usize = 8;

fn try_shutdown(stream: &TcpStream) {
    match stream.shutdown(Shutdown::Both) {
        Ok(()) => {}
        Err(e) => info!("[{:?}] attempted to shutdown stream but failed: {:?}", stream.local_addr(), e),
    }
}

/// Wrap a TcpStream into channels
fn wrap_tcpstream<S, R>(stream: TcpStream) -> (Sender<S>, Receiver<R>, Sender<()>, JoinHandle<()>)
where
    S: 'static + Sync + Send + Clone + Serialize,
    R: 'static + Sync + Send + Clone + DeserializeOwned,
{
    let (reader_s, reader_r) = bounded(TCPSTREAM_CAP);
    let (writer_s, writer_r) = bounded(TCPSTREAM_CAP);
    let (shutdown_s, shutdown_r) = bounded(1);
    let mut reader = stream.try_clone().unwrap();
    let mut writer = stream.try_clone().unwrap();

    let hdl = thread::spawn(move || {
        // read data from a stream and then forward it to a channel
        let read_hdl = thread::spawn(move || loop {
            let mut f = || -> Result<(), std::io::Error> {
                let mut length_buf = [0u8; LENGTH_BYTES];
                reader.read_exact(&mut length_buf)?;

                let n = usize::from_le_bytes(length_buf);
                let mut value_buf = vec![0u8; n];
                reader.read_exact(&mut value_buf)?;

                // we use expect here because we cannot recover from deserialzation failure
                let msg: R = bincode::deserialize(&value_buf).expect("deserialization failed");
                match reader_s.send(msg) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let custom_error = io::Error::new(io::ErrorKind::Other, e);
                        Err(custom_error)
                    }
                }
            };

            match f() {
                Ok(()) => {}
                Err(e) => {
                    info!("[{:?}] read failed but probably not an issue: {:?}", reader.local_addr(), e);
                    try_shutdown(&reader);
                    break;
                }
            }
        });

        // read data from a channel and then send it into a stream
        loop {
            select! {
                recv(writer_r) -> msg_res => {
                    let msg = msg_res.unwrap(); // TODO check unwrap
                    // construct a header with the length and concat it with the body
                    let mut data = bincode::serialized_size(&msg)
                        .expect("failed to find serialized size")
                        .to_le_bytes()
                        .to_vec();
                    // we use expect here because we cannot recover from serialization failure
                    data.extend(bincode::serialize(&msg).expect("serialization failed"));
                    match writer.write_all(&data) {
                        Ok(()) => {},
                        Err(e) => {
                            error!("[{:?}] write error: {:?}", writer.local_addr(), e);
                            try_shutdown(&writer);
                            break;
                        }
                    }
                }
                recv(shutdown_r) -> msg_res => {
                    msg_res.unwrap(); // TODO check unwrap
                    info!("[{:?}] closing stream with peer {:?}", writer.local_addr(), writer.peer_addr());
                    writer.shutdown(Shutdown::Both).expect("shutdown call failed");
                    info!("[{:?}] closed stream", writer.local_addr());
                    break;
                }
            }
        }
        read_hdl.join().expect("reader thread panicked")
    });

    (writer_s, reader_r, shutdown_s, hdl)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OnlineNode {
    id: PartyID,
    local_addr: SocketAddr,
    alpha_share: Fp,
    sync_addr: SocketAddr,
    preproc_addr: SocketAddr,
    peers: Vec<SocketAddr>,
}

impl OnlineNode {
    pub fn run(&self, prog: Vec<vm::Instruction>, reg: vm::Reg, seed: [usize; 4]) -> Result<Vec<Fp>, ApplicationError> {
        let listener = TcpListener::bind(self.local_addr)?;
        let mut peer_handlers = vec![];
        let mut preproc_items = None;
        let mut sync_items = None;

        let mut connected_peers = vec![];
        let mut peer_sender_chans = vec![];
        let mut peer_receiver_chans = vec![];
        let mut peer_shutdown_chans = vec![];

        for stream_result in listener.incoming() {
            if preproc_items.is_some() && sync_items.is_some() && connected_peers.len() == self.peers.len() {
                break;
            }

            let stream = stream_result?;
            match stream.peer_addr() {
                Ok(addr) => {
                    if addr == self.sync_addr && sync_items.is_none() {
                        let (s, r, shutdown_s, h) = wrap_tcpstream::<SyncReplyMsg, SyncMsg>(stream);
                        sync_items = Some((s, r, shutdown_s, h));
                    } else if addr == self.preproc_addr && preproc_items.is_none() {
                        let (s, r, shutdown_s, h) = wrap_tcpstream::<PreprocMsg, PreprocMsg>(stream);
                        preproc_items = Some((s, r, shutdown_s, h));
                    } else if self.peers.contains(&addr) && !connected_peers.contains(&addr) {
                        let (s, r, shutdown_s, h) = wrap_tcpstream::<PartyMsg, PartyMsg>(stream);
                        peer_sender_chans.push(s);
                        peer_receiver_chans.push(r);
                        peer_shutdown_chans.push(shutdown_s);
                        peer_handlers.push(h);
                        connected_peers.push(addr);
                    } else {
                        try_shutdown(&stream);
                        continue;
                    }
                }
                Err(e) => {
                    error!("[{:?}] unable to get peer address: {:?}", stream.local_addr(), e);
                    continue;
                }
            };
        }

        // start the party
        let (sync_sender_chan, sync_receiver_chan, sync_shutdown_chan, sync_handler) = sync_items.unwrap();
        let (_preproc_sender_chan, preproc_receiver_chan, preproc_shutdown_chan, preproc_handler) = preproc_items.unwrap();
        let party_handler = party::Party::spawn(
            self.id, 
            self.alpha_share.clone(),
            reg,
            prog,
            sync_sender_chan,
            sync_receiver_chan,
            preproc_receiver_chan,
            peer_sender_chans,
            peer_receiver_chans,
            seed);

        // join the threads when the party is done
        let res = party_handler.join().unwrap()?; // TODO should we unwrap?
        sync_shutdown_chan.send(()).unwrap();
        preproc_shutdown_chan.send(()).unwrap();
        for c in peer_shutdown_chans {
            c.send(()).unwrap();
        }

        // TODO what to do with these unwrap?
        sync_handler.join().unwrap();
        preproc_handler.join().unwrap();
        for h in peer_handlers {
            h.join().unwrap();
        }
        Ok(res)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use test_env_log::test;

    #[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
    struct Msg {
        a: usize,
    }

    #[test]
    fn test_tcpstream_wrapper() {
        // TODO this test is flakey, find out why.
        const ADDR: &str = "127.0.0.1:36794";
        const MSG1: Msg = Msg { a: 1 };
        const MSG2: Msg = Msg { a: 2 };

        // start server in a thread
        let (s, r) = bounded(1);
        let server_hdl: JoinHandle<Msg> = thread::spawn(move || {
            let listener = TcpListener::bind(ADDR).unwrap();
            s.send(()).unwrap();
            let (mut stream, _) = listener.accept().unwrap();

            // write a message
            let mut msg1_buf = bincode::serialized_size(&MSG1).unwrap().to_le_bytes().to_vec();
            msg1_buf.extend(bincode::serialize(&MSG1).unwrap());
            stream.write_all(&msg1_buf).unwrap();

            // read a message
            let mut read_len_buf = [0u8; LENGTH_BYTES];
            stream.read_exact(&mut read_len_buf).unwrap();
            let read_len = usize::from_le_bytes(read_len_buf);

            let mut read_buf = vec![0u8; read_len];
            stream.read_exact(&mut read_buf).unwrap();
            s.send(()).unwrap();
            bincode::deserialize(&read_buf).unwrap()
        });

        // wait for server to start and get a client stream
        assert_eq!((), r.recv().unwrap());
        let stream = TcpStream::connect(ADDR).unwrap();

        // test the wrapper, first receive the first message from server
        let (sender, receiver, shutdown_sender, handle) = wrap_tcpstream::<Msg, Msg>(stream);
        let msg1: Msg = receiver.recv().unwrap();
        assert_eq!(msg1, MSG1);

        // send MSG2 and send a close message
        sender.send(MSG2).unwrap();
        assert_eq!((), r.recv().unwrap());
        shutdown_sender.send(()).unwrap();

        assert_eq!(server_hdl.join().unwrap(), MSG2);
        handle.join().unwrap()
    }
}
