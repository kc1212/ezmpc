use bincode;
use crossbeam::channel::{bounded, select, Receiver, Sender};
use log::{error, info};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::thread::JoinHandle;
use std::{io, unimplemented};

use crate::algebra::Fp;
use crate::error::ApplicationError;
use crate::message::PartyID;

const SHIM_CAP: usize = 1000;
const LENGTH_BYTES: usize = 8;

fn try_shutdown(stream: &TcpStream) {
    match stream.shutdown(Shutdown::Both) {
        Ok(()) => {}
        Err(e) => info!("[{:?}] attempted to shutdown stream but failed: {:?}", stream.local_addr(), e),
    }
}

/// Wrap a TcpStream into channels.
pub fn wrap_tcpstream<S, R>(stream: TcpStream) -> (Sender<Option<S>>, Receiver<R>, JoinHandle<()>)
where
    S: 'static + Sync + Send + Clone + Serialize,
    R: 'static + Sync + Send + Clone + DeserializeOwned,
{
    let (reader_s, reader_r) = bounded(SHIM_CAP);
    let (writer_s, writer_r) = bounded(SHIM_CAP);
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
                    match msg {
                        None => {
                            info!("[{:?}] closing stream with peer {:?}", writer.local_addr(), writer.peer_addr());
                            writer.shutdown(Shutdown::Both).expect("shutdown call failed");
                            info!("[{:?}] closed stream", writer.local_addr());
                            break;
                        }
                        Some(m) => {
                            // construct a header with the length and concat it with the body
                            let mut data = bincode::serialized_size(&m)
                                .expect("failed to find serialized size")
                                .to_le_bytes()
                                .to_vec();
                            // we use expect here because we cannot recover from serialization failure
                            data.extend(bincode::serialize(&m).expect("serialization failed"));
                            match writer.write_all(&data) {
                                Ok(()) => {},
                                Err(e) => {
                                    error!("[{:?}] write error: {:?}", writer.local_addr(), e);
                                    try_shutdown(&writer);
                                    break;
                                }
                            }
                        }
                    }
                },
            }
        }
        read_hdl.join().expect("reader thread panicked")
    });

    (writer_s, reader_r, hdl)
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
    fn run(&self) -> Result<(), ApplicationError> {
        // let listener = TcpListener::bind(self.local_addr)?;
        // let mut handlers = vec![];
        // for stream_result in listener.incoming() {
        //     let stream = stream_result?;

        //     // check if such an IP already exists, otherwise drop the connections
        //     match stream.peer_addr() {
        //         Ok(addr) => {
        //             if self.peers.contains(&addr) {
        //                 // TODO drop connection
        //                 continue;
        //             }
        //         }
        //         Err(e) => {
        //             // TODO report error
        //             continue;
        //         }
        //     };

        //     // handle the stream
        //     let (s, r, h) = wrap_tcpstream(stream);
        //     handlers.push(h);
        // }
        // Ok(())
        unimplemented!()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use test_env_log::test;

    #[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
    struct Msg {
        a: usize,
        close: bool,
    }

    #[test]
    fn test_tcpstream_wrapper() {
        const ADDR: &str = "127.0.0.1:36794";
        const MSG1: Msg = Msg { a: 1, close: false };
        const MSG2: Msg = Msg { a: 2, close: false };

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
        let (sender, receiver, handle) = wrap_tcpstream(stream);
        let msg1: Msg = receiver.recv().unwrap();
        assert_eq!(msg1, MSG1);

        // send MSG2 and send a close message
        sender.send(Some(MSG2)).unwrap();
        assert_eq!((), r.recv().unwrap());
        sender.send(None).unwrap();

        assert_eq!(server_hdl.join().unwrap(), MSG2);
        handle.join().unwrap()
    }
}

/*
use crate::{algebra::Fp, crypto::commit, error::ApplicationError, message::Msg};
use crate::party::Party;

use crossbeam::channel::{bounded, select};
use serde::{Serialize, Deserialize};
use std::net::{TcpListener, TcpStream, SocketAddr};
use std::thread;
use std::thread::JoinHandle;
use std::fs::File;
use std::io::{Write, Read};

fn tlv_encode(msg: &Msg) -> Vec<u8> {
    todo!()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Node {
    id: PartyID,
    addr: SocketAddr,
    sync_addr: SocketAddr,
    alpha_share: Fp,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PublicIdentity {
    addr: SocketAddr,
}

fn spawn_handler(stream: TcpStream) -> JoinHandle<()> {
    let (reader_s, reader_r) = bounded(1000); // TODO bound?
    let mut reader = stream.try_clone().unwrap();
    let mut writer = stream.try_clone().unwrap();

    let write_hdl = thread::spawn(move || {
        // spawn the read thread
        let read_hdl = thread::spawn(move || {
            loop {
                // TODO handle end of stream
                let mut buf = vec![];
                let n = reader.read(&mut buf).unwrap();
                reader_s.send(buf).unwrap();
            }
        });

        loop {
            select! {
                recv(reader_r) -> buf => {
                    todo!()
                },
            }
        }

        read_hdl.join().unwrap()
    });

    write_hdl
}

impl Node {
    fn new(private_toml: &str, public_toml: &str) -> Result<Node, ApplicationError> {
        let mut f = File::open(private_toml)?;
        let mut content = String::new();
        f.read_to_string(&mut content)?;

        let node: Node = toml::from_str(&content)?;
        Ok(node)
    }

    fn listen(&self) -> Result<(), ApplicationError> {
        let party = Party {
            id: self.id,
            alpha_share: self.alpha_share,
            com_scheme: commit::Scheme{},
            s_sync_chan: (),
            r_sync_chan: (),
            triple_chan: (),
            rand_chan: (),
            s_party_chans: (),
            r_party_chans: (),
        };

        let listener = TcpListener::bind(self.addr)?;
        let nodes: Vec<PublicIdentity> = vec![];
        let mut handlers = vec![];
        for stream in listener.incoming() {
            // check if such an IP already exists, otherwise drop the connections
            let h = spawn_handler(stream?);
            handlers.push(h);
        }
        // TODO consider using waitgroup
        Ok(())
    }
}
*/
