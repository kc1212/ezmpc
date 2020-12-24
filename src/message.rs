use crossbeam_channel::{Sender, Receiver, SendError, RecvTimeoutError};
use std::time::Duration;
use log::debug;
use std::fmt::Debug;

pub fn broadcast<T: Copy + Clone + Debug>(o_chans: &Vec<Sender<T>>, m: T) -> Result<(), SendError<T>> {
    debug!("Broadcasting {:?}", m);
    for c in o_chans {
        c.send(m)?;
    }
    Ok(())
}

pub fn recv_all<T: Copy + Clone + Debug>(i_chans: &Vec<Receiver<T>>) -> Result<Vec<T>, RecvTimeoutError> {
    let mut out: Vec<T> = Vec::new();
    for c in i_chans {
        let m = c.recv_timeout(Duration::from_secs(1))?;
        out.push(m);
    }
    debug!("All received {:?}", out);
    Ok(out)
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SyncMsg {
    Start,
    Next,
    Abort,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SyncMsgReply {
    Ok,
    Done,
    Abort,
}

