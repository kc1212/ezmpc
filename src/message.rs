use crate::algebra::Fp;

use crate::crypto::commit;
use crossbeam_channel::{Receiver, RecvTimeoutError, SendError, Sender};
use log::debug;
use std::fmt::Debug;
use std::time::Duration;

pub(crate) fn broadcast<T: Copy + Clone + Debug>(s_chans: &Vec<Sender<T>>, m: T) -> Result<(), SendError<T>> {
    debug!("Broadcasting {:?}", m);
    for c in s_chans {
        c.send(m)?;
    }
    Ok(())
}

pub(crate) fn recv_all<T: Copy + Clone + Debug>(r_chans: &Vec<Receiver<T>>) -> Result<Vec<T>, RecvTimeoutError> {
    let mut out: Vec<T> = Vec::new();
    for c in r_chans {
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

#[derive(Copy, Clone, Debug)]
pub enum NodeMsg {
    Elem(Fp),
    Com(commit::Commitment),
    Opening(commit::Opening),
}
