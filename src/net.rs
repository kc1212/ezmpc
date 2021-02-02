use crossbeam::channel::{bounded, select, Receiver, Sender};
use log::{info, error};
use std::io;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::thread;
use std::thread::JoinHandle;

const SHIM_CAP: usize = 1000;
const TYPE_BYTES: usize = 8;
const LENGTH_BYTES: usize = 8;

/// Wrap a TcpStream into channels.
pub fn wrap_tcpstream(stream: TcpStream) -> (Sender<Vec<u8>>, Receiver<([u8; 8], Vec<u8>)>, JoinHandle<()>) {
    let (reader_s, reader_r) = bounded(SHIM_CAP);
    let (writer_s, writer_r): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = bounded(SHIM_CAP);
    let mut reader = stream.try_clone().unwrap();
    let mut writer = stream.try_clone().unwrap();

    let hdl = thread::spawn(move || {
        // read data from a stream and then forward it to a channel
        let read_hdl = thread::spawn(move || {
            loop {
                let mut f = || -> Result<(), std::io::Error> {
                    let mut type_buf: [u8; TYPE_BYTES] = [0; TYPE_BYTES];
                    let mut length_buf: [u8; LENGTH_BYTES] = [0; LENGTH_BYTES];
                    reader.read_exact(&mut type_buf)?;
                    reader.read_exact(&mut length_buf)?;

                    let n = usize::from_le_bytes(length_buf);
                    let mut value_buf = vec![0; n];
                    reader.read_exact(&mut value_buf)?;

                    match reader_s.send((type_buf, value_buf)) {
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
                        info!("[-] read failed but probably not an issue: {:?}", e);
                        match reader.shutdown(Shutdown::Both) {
                            Ok(()) => {},
                            Err(e) => info!("[{:?}] attempted to close stream but failed: {:?}", reader.local_addr(), e),
                        }
                        break;
                    }
                }
            }
        });

        // read data from a channel and then send it into a stream
        loop {
            select! {
                recv(writer_r) -> buf => {
                    let v = buf.unwrap();
                    if v.is_empty() {
                        // is there another way to close the stream?
                        info!("[{:?}] closing stream with peer {:?}", writer.local_addr(), writer.peer_addr());
                        writer.shutdown(Shutdown::Both).expect("shutdown call failed");
                        break;
                    }
                    match writer.write_all(&v) {
                        Ok(()) => {},
                        Err(e) => {
                            error!("[{:?}] write error: {:?}", writer.local_addr(), e);
                            writer.shutdown(Shutdown::Both).expect("shutdown call failed");
                            break;
                        }
                    }
                },
            }
        }
        read_hdl.join().unwrap()
    });

    (writer_s, reader_r, hdl)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn test_tcpstream_wrapper() {
        // start server in a thread
        const ADDR: &str = "127.0.0.1:36794";
        const TYPE: usize = 0;
        const MSG: [u8; 4] = [1,2,3,4];
        const MSG2: [u8; 1] = [9];
        let (s, r) = bounded(1);
        let server_hdl = thread::spawn(move || {
            let listener = TcpListener::bind(ADDR).unwrap();
            s.send(()).unwrap();
            let (mut stream, _) = listener.accept().unwrap();

            // write a message
            let mut buf = [TYPE.to_le_bytes(), MSG.len().to_le_bytes()].concat();
            buf.extend_from_slice(&MSG);
            stream.write_all(&buf).unwrap();

            // read a message
            let mut read_buf = [0u8; MSG2.len()];
            stream.read_exact(&mut read_buf).unwrap();
            read_buf
        });

        // wait for server to start and get a client stream
        assert_eq!((), r.recv().unwrap());
        let stream = TcpStream::connect(ADDR).unwrap();

        // test the wrapper
        let (sender, receiver, handle) = wrap_tcpstream(stream);
        let (type_buf, msg_buf) = receiver.recv().unwrap();
        assert_eq!(type_buf, TYPE.to_le_bytes());
        assert_eq!(msg_buf, MSG.to_vec());

        sender.send(MSG2.into()).unwrap();
        sender.send(vec![]).unwrap(); // this line means close the stream

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
