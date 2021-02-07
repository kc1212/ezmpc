use bincode;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use crossbeam::channel::{bounded, select, Receiver, Sender};
use log::{debug, error, info};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::read_to_string;
use std::io;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::algebra::Fp;
use crate::error::ApplicationError;
use crate::message::*;
use crate::synchronizer;

const TCPSTREAM_CAP: usize = 1000;
const LENGTH_BYTES: usize = 8;
const FORM_CLUSTER: u8 = 42;
const FORM_CLUSTER_ACK: u8 = 41;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NodeConf {
    pub addr: SocketAddr,
    pub id: PartyID,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PublicConf {
    pub sync_addr: SocketAddr,
    pub nodes: Vec<NodeConf>,
}

impl PublicConf {
    pub fn arg_name() -> &'static str {
        "PUBLIC_CONFIG"
    }

    pub fn from_file(f: &str) -> Result<PublicConf, io::Error> {
        let s = read_to_string(f)?;
        ron::from_str(&s).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PrivateConf {
    pub id: PartyID,
    pub listen_addr: SocketAddr,
    #[serde(with = "fp_serde")]
    pub alpha_share: Fp,
}

mod fp_serde {
    use crate::algebra::Fp;

    use serde::{de, de::Visitor, Deserializer, Serializer};
    use std::fmt;
    use std::str::FromStr;

    pub(crate) fn serialize<S>(fp: &Fp, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_str(&fp.to_string())
    }

    struct FpVisitor;
    impl<'de> Visitor<'de> for FpVisitor {
        type Value = Fp;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a field element")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Fp::from_str(v).map_err(|e| de::Error::custom(e))
        }
    }

    pub(crate) fn deserialize<'de, D>(d: D) -> Result<Fp, D::Error>
    where
        D: Deserializer<'de>,
    {
        d.deserialize_str(FpVisitor)
    }
}

impl PrivateConf {
    pub fn arg_name() -> &'static str {
        "PRIVATE_CONFIG"
    }

    pub fn from_file(f: &str) -> Result<PrivateConf, io::Error> {
        let s = read_to_string(f)?;
        ron::from_str(&s).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SynchronizerConfig {
    pub listen_addr: SocketAddr,
}

impl SynchronizerConfig {
    pub fn arg_name() -> &'static str {
        "SYNCHRONIZER_CONFIG"
    }

    pub fn from_file(f: &str) -> Result<SynchronizerConfig, io::Error> {
        let s = read_to_string(f)?;
        ron::from_str(&s).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

fn pp(x: &io::Result<SocketAddr>) -> String {
    match x {
        Ok(addr) => addr.to_string(),
        Err(_) => "xxxx:xxxx".to_string(),
    }
}

fn try_shutdown(stream: &TcpStream) {
    match stream.shutdown(Shutdown::Both) {
        Ok(()) => info!("[{}] shutdown ok", pp(&stream.local_addr())),
        Err(e) => info!("[{}] attempted to shutdown stream but failed: {:?}", pp(&stream.local_addr()), e),
    }
}

/// Discover other nodes in the system.
/// This function should be used by the synchronizer who coordinates all the communication.
/// The synchronizer should start as the first node.
/// Every other node connects to the synchronizer.
/// When all the nodes are online, the synchronizer sends a "form cluster" command to all other nodes.
/// TODO: use TLS
fn start_discovery(listen_addr: SocketAddr, target_ids: &Vec<PartyID>) -> Result<HashMap<PartyID, TcpStream>, io::Error> {
    let mut out: HashMap<PartyID, TcpStream> = HashMap::new();
    let listener = TcpListener::bind(listen_addr)?;
    for stream_res in listener.incoming() {
        let mut stream = stream_res?;
        info!("[{}] found peer {}", pp(&listener.local_addr()), pp(&stream.peer_addr()));

        let candidate_id = stream.read_u32::<LittleEndian>()?;
        if !out.contains_key(&candidate_id) && target_ids.contains(&candidate_id) {
            out.insert(candidate_id, stream);
        } else {
            info!("[{}] shutting down bad peer with id {}", pp(&listener.local_addr()), candidate_id);
            stream.shutdown(Shutdown::Both)?;
        }

        if out.len() == target_ids.len() {
            info!("[{}] all peers connected, sending 'form cluster' command", pp(&listener.local_addr()));
            break;
        }
    }

    for stream in out.values_mut() {
        stream.write_u8(FORM_CLUSTER)?;
    }

    // and we expect an 'ACK'
    for stream in out.values_mut() {
        let x = stream.read_u8()?;
        if x != FORM_CLUSTER_ACK {
            error!("[{}] ACK is wrong from {}", pp(&listener.local_addr()), pp(&stream.peer_addr()))
        }
    }
    info!("[{:?}] 'form cluster' message sent", listener.local_addr());
    Ok(out)
}

/// Connect to the discovery and wait for the 'form cluster' message.
/// Retruns a TcpStream that is connected to the synchronizer.
fn wait_start(sync_addr: SocketAddr, my_id: PartyID) -> Result<TcpStream, io::Error> {
    let mut stream = retry_connection(sync_addr, 1000, Duration::from_millis(500))?;
    stream.write_u32::<LittleEndian>(my_id)?;
    let signal = stream.read_u8()?;
    if signal == FORM_CLUSTER {
        stream.write_u8(FORM_CLUSTER_ACK)?;
        Ok(stream)
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, "invalid 'form cluster' signal"))
    }
}

/// Listen for new connections but do not accept until `wait_start` unblocks.
/// Then, accept connections from IDs that are lower than `my_id`.
/// Make TCP connections to IDs that are higher than mine.
/// If there are none, do not make TCP connections.
fn form_cluster(listener: TcpListener, my_id: PartyID, all_nodes: &Vec<NodeConf>) -> Result<HashMap<PartyID, TcpStream>, io::Error> {
    // spawn a thread to accept valid connections
    let all_ids: Vec<PartyID> = all_nodes.iter().map(|x| x.id).collect();
    let ids_to_connect: Vec<PartyID> = all_ids.clone().into_iter().filter(|id| *id < my_id).collect();
    let ids_to_receive: Vec<PartyID> = all_ids.clone().into_iter().filter(|id| *id > my_id).collect();
    debug!("[{:?}] node {} waiting for ids {:?}", listener.local_addr(), my_id, ids_to_receive);
    debug!("[{:?}] node {} connecting to ids {:?}", listener.local_addr(), my_id, ids_to_connect);

    let handler = thread::spawn(move || {
        let mut out: HashMap<PartyID, TcpStream> = HashMap::new();
        if ids_to_receive.is_empty() {
            return out;
        }

        for stream_res in listener.incoming() {
            match stream_res {
                Ok(mut stream) => {
                    let candidate_id = stream.read_u32::<LittleEndian>().expect("cannot read u32");
                    if ids_to_receive.contains(&candidate_id) && !out.contains_key(&candidate_id) {
                        #[rustfmt::skip]
                        debug!("[{}] received candidate {} from {}", 
                               pp(&listener.local_addr()), candidate_id, pp(&stream.peer_addr()));
                        out.insert(candidate_id, stream);
                    } else {
                        #[rustfmt::skip]
                        error!("[{}] received invalid id {:?} from {}", 
                               pp(&listener.local_addr()), candidate_id, pp(&stream.peer_addr()));
                    }
                }
                Err(e) => {
                    error!("[{}] connection issue: {:?}", pp(&listener.local_addr()), e);
                }
            }

            if out.len() == ids_to_receive.len() {
                info!("[{}] received all connections", pp(&listener.local_addr()));
                break;
            }
        }
        out
    });

    // make connections to the IDs that are higher than mine
    let mut out: HashMap<PartyID, TcpStream> = HashMap::new();
    for node in all_nodes {
        if ids_to_connect.contains(&node.id) && !out.contains_key(&node.id) {
            let mut stream = retry_connection(node.addr, 20, Duration::from_millis(200))?;
            stream.write_u32::<LittleEndian>(my_id)?;
            out.insert(node.id, stream);
        }
    }

    // combine the two
    let others = handler.join().expect("form cluster thread panicked");
    out.extend(others);
    std::assert_eq!(out.len(), all_nodes.len() - 1);
    debug!("[xxxx:xxxx] {} cluster formation ok", my_id);
    Ok(out)
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
                    info!("[{}] read failed but probably not an issue: {:?}", pp(&reader.local_addr()), e);
                    // try to shutdown because the writer might've closed the stream too
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
                            error!("[{}] write error: {:?}", pp(&writer.local_addr()), e);
                            try_shutdown(&writer);
                            break;
                        }
                    }
                }
                recv(shutdown_r) -> msg_res => {
                    msg_res.unwrap(); // TODO check unwrap
                    info!("[{}] closing stream with peer {}", pp(&writer.local_addr()), pp(&writer.peer_addr()));
                    // try to shutdown because the reader might've closed the stream too
                    try_shutdown(&writer);
                    break;
                }
            }
        }
        read_hdl.join().expect("reader thread panicked")
    });

    (writer_s, reader_r, shutdown_s, hdl)
}

fn retry_connection(addr: SocketAddr, tries: usize, interval: Duration) -> Result<TcpStream, io::Error> {
    let mut last_error = io::Error::new(io::ErrorKind::Other, "dummy error");
    for _ in 0..tries {
        match TcpStream::connect(addr.clone()) {
            Ok(stream) => {
                return Ok(stream);
            }
            Err(e) => {
                last_error = e;
                thread::sleep(interval);
            }
        }
    }
    Err(last_error)
}

pub fn synchronizer_main(public_conf: PublicConf, synchronizer_conf: SynchronizerConfig) -> Result<(), ApplicationError> {
    let ids: Vec<PartyID> = public_conf.nodes.clone().iter().map(|x| x.id).collect();
    let stream_map = start_discovery(synchronizer_conf.listen_addr, &ids)?;

    let mut peer_handlers = vec![];
    let mut peer_sender_chans = vec![];
    let mut peer_receiver_chans = vec![];
    let mut peer_shutdown_chans = vec![];

    for (_id, stream) in stream_map {
        let (s, r, shutdown_s, h) = wrap_tcpstream::<SyncMsg, SyncReplyMsg>(stream);
        peer_sender_chans.push(s);
        peer_receiver_chans.push(r);
        peer_shutdown_chans.push(shutdown_s);
        peer_handlers.push(h);
    }

    let sync_handle = synchronizer::Synchronizer::spawn(peer_sender_chans, peer_receiver_chans);
    sync_handle.join().expect("synchronizer thread panicked")?;
    for h in peer_handlers {
        h.join().unwrap();
    }
    Ok(())
}

pub fn online_node_main(public_conf: PublicConf, private_conf: PrivateConf) -> Result<(), ApplicationError> {
    let listener = TcpListener::bind(private_conf.listen_addr)?;
    let sync_stream = wait_start(public_conf.sync_addr, private_conf.id)?;
    let (sync_s, sync_r, sync_shutdown, sync_h) = wrap_tcpstream::<SyncReplyMsg, SyncMsg>(sync_stream);

    let stream_map = form_cluster(listener, private_conf.id, &public_conf.nodes)?;

    let mut peer_handlers = vec![];
    let mut peer_sender_chans = vec![];
    let mut peer_receiver_chans = vec![];
    let mut peer_shutdown_chans = vec![];

    for (_id, stream) in stream_map {
        let (s, r, shutdown_s, h) = wrap_tcpstream::<PartyMsg, PartyMsg>(stream);
        peer_sender_chans.push(s);
        peer_receiver_chans.push(r);
        peer_shutdown_chans.push(shutdown_s);
        peer_handlers.push(h);
    }

    // let party_handle = Party::spawn(private_conf.id, private_conf.alpha_share.clone(), )
    // TODO

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crossbeam;
    use ron;
    use test_env_log::test;

    #[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
    struct Msg {
        a: usize,
    }

    #[test]
    fn test_tcpstream_wrapper() {
        const ADDR: &str = "127.0.0.1:36794"; // consider using port 0 as wildcard
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

    #[test]
    fn test_public_conf() -> Result<(), io::Error> {
        let ron_str = read_to_string("conf/public.ron")?;
        let public_conf: PublicConf = ron::from_str(&ron_str).unwrap();
        assert_eq!(public_conf.sync_addr, "[::1]:12345".parse().unwrap());
        assert_eq!(public_conf.nodes.len(), 3);
        assert_eq!(public_conf.nodes[0].addr, "[::1]:14270".parse().unwrap());
        assert_eq!(public_conf.nodes[0].id, 0);
        Ok(())
    }

    #[test]
    fn test_synchronizer_conf() -> Result<(), io::Error> {
        let ron_str = read_to_string("conf/synchronizer.ron")?;
        let public_conf: SynchronizerConfig = ron::from_str(&ron_str).unwrap();
        assert_eq!(public_conf.listen_addr, "[::1]:12345".parse().unwrap());
        Ok(())
    }

    #[test]
    fn test_private_conf() -> Result<(), io::Error> {
        {
            let ron_str = read_to_string("conf/private_0.ron")?;
            let private_conf: PrivateConf = ron::from_str(&ron_str).unwrap();
            assert_eq!(private_conf.id, 0);
            assert_eq!(private_conf.listen_addr, "[::1]:14270".parse().unwrap());
            assert_eq!(private_conf.alpha_share.to_string(), "pv///1kAAABaqJMA8lZUjyAOGp0sDar9nfShXsMscz4=");
        }
        {
            let ron_str = read_to_string("conf/private_1.ron")?;
            let private_conf: PrivateConf = ron::from_str(&ron_str).unwrap();
            assert_eq!(private_conf.id, 1);
            assert_eq!(private_conf.listen_addr, "[::1]:14271".parse().unwrap());
            assert_eq!(private_conf.alpha_share.to_string(), "ngS++JPaVER19LutvDJ9jvbvY2jAD3034Ql2d4InXz8=");
        }
        {
            let ron_str = read_to_string("conf/private_2.ron")?;
            let private_conf: PrivateConf = ron::from_str(&ron_str).unwrap();
            assert_eq!(private_conf.id, 2);
            assert_eq!(private_conf.listen_addr, "[::1]:14272".parse().unwrap());
            assert_eq!(private_conf.alpha_share.to_string(), "3gvur5vUxXSXFe+R0cd8QWhmN6rUMgDhIPWmvXnU714=");
        }
        Ok(())
    }

    #[test]
    fn test_discovery() -> Result<(), io::Error> {
        let listen_addr: SocketAddr = "[::1]:12345".parse().unwrap();
        let target_ids: Vec<PartyID> = vec![0, 1];
        let sync_handler = thread::spawn(move || start_discovery(listen_addr, &target_ids));

        let mut client_bad = retry_connection(listen_addr, 10, Duration::from_millis(100))?;
        client_bad.write_u32::<LittleEndian>(2)?;
        client_bad.read_u8().expect_err("remote should close connection with bad party ID");

        let mut client0 = TcpStream::connect(listen_addr)?;
        let mut client1 = TcpStream::connect(listen_addr)?;

        client0.write_u32::<LittleEndian>(0)?;
        client1.write_u32::<LittleEndian>(1)?;

        let v0 = client0.read_u8()?;
        let v1 = client1.read_u8()?;
        assert_eq!(FORM_CLUSTER, v0);
        assert_eq!(FORM_CLUSTER, v1);

        client0.write_u8(FORM_CLUSTER_ACK)?;
        client1.write_u8(FORM_CLUSTER_ACK)?;

        let mut res = sync_handler.join().expect("discovery thread panicked")?;
        for stream in res.values_mut() {
            stream.write_u8(88)?;
        }

        let w0 = client0.read_u8()?;
        let w1 = client1.read_u8()?;
        assert_eq!(88, w0);
        assert_eq!(88, w1);
        Ok(())
    }

    #[test]
    fn test_cluster_formation() -> Result<(), io::Error> {
        #[rustfmt::skip]
            let nodes = vec![
            NodeConf { addr: "[::1]:9000".parse().unwrap(), id: 0 },
            NodeConf { addr: "[::1]:9111".parse().unwrap(), id: 1 },
            NodeConf { addr: "[::1]:9222".parse().unwrap(), id: 2 },
        ];
        let ids: Vec<PartyID> = nodes.clone().iter().map(|x| x.id).collect();

        // NOTE socket address must not be reused in test otherwise it'll conflict with other tests
        // since cargo test runs them in parallel
        let sync_addr: SocketAddr = "[::1]:12347".parse().unwrap();
        let synchronizer_handler = thread::spawn(move || start_discovery(sync_addr, &ids));

        // use a waitgroup to wait for the synchronizer to announce 'form cluster'
        let wg = crossbeam::sync::WaitGroup::new();
        let mut listeners = vec![];
        for node in &nodes {
            listeners.push(TcpListener::bind(node.addr)?);
            let wg = wg.clone();
            let id = node.id;
            let sync_addr = sync_addr.clone();
            thread::spawn(move || {
                let _ = wait_start(sync_addr, id).unwrap();
                drop(wg);
            });
        }
        wg.wait();

        // the nodes start to form cluster
        let mut handlers = vec![];
        let nodes_copy = nodes.clone();
        for (node, listener) in nodes.iter().zip(listeners) {
            let id = node.id;
            let nodes_copy = nodes_copy.clone(); // is there a way to avoid multiple clone?
            let h = thread::spawn(move || form_cluster(listener, id, &nodes_copy).expect("form cluster thread panicked"));
            handlers.push(h);
        }

        let mut stream_maps = Vec::new();
        for h in handlers {
            let stream_map = h.join().unwrap();
            stream_maps.push(stream_map);
        }
        let _ = synchronizer_handler.join().expect("synchronizer thread panicked");

        // sending a message from one node should be received by another
        let x = 66u8;
        stream_maps.get_mut(0).unwrap().get_mut(&1).unwrap().write_u8(x)?;
        let xx = stream_maps.get_mut(1).unwrap().get_mut(&0).unwrap().read_u8()?;
        assert_eq!(x, xx);

        let y = 77u8;
        stream_maps.get_mut(1).unwrap().get_mut(&2).unwrap().write_u8(y)?;
        let yy = stream_maps.get_mut(2).unwrap().get_mut(&1).unwrap().read_u8()?;
        assert_eq!(y, yy);

        // the synchronizer should not be listening anymore
        TcpStream::connect(sync_addr).expect_err("synchronizer should not be listening");
        Ok(())
    }
}
