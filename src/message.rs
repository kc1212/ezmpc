use crate::algebra::Fp;
use crate::crypto;
use crate::crypto::commit;

use crossbeam_channel::{Receiver, RecvTimeoutError, SendError, Sender};
use log::debug;
use std::fmt::Debug;
use std::time::Duration;

pub type PartyID = usize;

/// Broadcast a message of type `T` to all the channels in `s_chans`.
pub(crate) fn broadcast<T: Copy + Clone + Debug>(s_chans: &Vec<Sender<T>>, m: T) -> Result<(), SendError<T>> {
    debug!("Broadcasting {:?}", m);
    for c in s_chans {
        c.send(m)?;
    }
    Ok(())
}

/// Wait for one message of type `T` from every channel in `r_chans`.
pub(crate) fn recv_all<T: Copy + Clone + Debug>(r_chans: &Vec<Receiver<T>>, dur: Duration) -> Result<Vec<T>, RecvTimeoutError> {
    let mut out: Vec<T> = Vec::new();
    for c in r_chans {
        let m = c.recv_timeout(dur)?;
        out.push(m);
    }
    debug!("All received {:?}", out);
    Ok(out)
}

/// This is the message sent, usually using broadcast,
/// by the synchronizer to the individual nodes.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SyncMsg {
    Start,
    Next,
    Abort,
}

/// This is the message send from the nodes to the synchronizer.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SyncReplyMsg {
    Ok,
    Done,
    Abort,
}

/// This is the message sent between the nodes themselves.
#[derive(Copy, Clone, Debug)]
pub enum PartyMsg {
    Elem(Fp),
    Com(commit::Commitment),
    Opening(commit::Opening),
}

/// This is a share of a Beaver triple where `a * b = c`,
/// used for computing multiplication.
pub struct TripleMsg {
    pub a: crypto::AuthShare,
    pub b: crypto::AuthShare,
    pub c: crypto::AuthShare,
}

impl TripleMsg {
    pub fn new(a: crypto::AuthShare, b: crypto::AuthShare, c: crypto::AuthShare) -> TripleMsg {
        TripleMsg { a, b, c }
    }
}

/// This is a random sharing where only one party knows the random share,
/// used for inputting a secret value into the MPC.
#[derive(Copy, Clone, Debug)]
pub struct RandShareMsg {
    pub share: crypto::AuthShare,
    pub clear: Option<Fp>,
    pub party_id: PartyID,
}

// TODO define a type for internal message, between vm and node
