//! Here are some structs and enums that represent message that can be sent through channels.

use crate::algebra::Fp;
use crate::crypto;
use crate::crypto::commit;

use crossbeam::channel::{Receiver, RecvTimeoutError, SendError, Sender};
use log::debug;
use std::fmt::Debug;
use std::time::Duration;

pub type PartyID = usize;

/// Broadcast a message of type `T` to all the channels in `s_chans`.
pub(crate) fn broadcast<T: Clone + Debug>(s_chans: &Vec<Sender<T>>, m: T) -> Result<(), SendError<T>> {
    debug!("Broadcasting {:?}", m);
    for c in s_chans {
        c.send(m.clone())?;
    }
    Ok(())
}

/// Wait for one message of type `T` from every channel in `r_chans`.
pub(crate) fn receive<T: Clone + Debug>(r_chans: &Vec<Receiver<T>>, dur: Duration) -> Result<Vec<T>, RecvTimeoutError> {
    let mut out: Vec<T> = Vec::new();
    for c in r_chans {
        let m = c.recv_timeout(dur)?;
        out.push(m);
    }
    debug!("All received {:?}", out);
    Ok(out)
}

pub enum Msg {}

/// This is the message sent, usually using broadcast,
/// by the synchronizer to the individual parties.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum SyncMsg {
    Start,
    Next,
    Abort,
}

/// This is the message send from the parties to the synchronizer.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum SyncReplyMsg {
    Ok,
    Done,
    Abort,
}

/// This is the message sent between the parties themselves.
#[derive(Clone, Debug)]
pub enum PartyMsg {
    Elem(Fp),
    Com(commit::Commitment),
    Opening(commit::Opening),
}

impl PartyMsg {
    pub(crate) fn unwrap_elem(self) -> Fp {
        match self {
            PartyMsg::Elem(x) => x,
            e => panic!("expected elem, got {:?}", e),
        }
    }

    pub(crate) fn unwrap_com(self) -> commit::Commitment {
        match self {
            PartyMsg::Com(x) => x,
            e => panic!("expected com, got {:?}", e),
        }
    }

    pub(crate) fn unwrap_opening(self) -> commit::Opening {
        match self {
            PartyMsg::Opening(x) => x,
            e => panic!("expected opening, got {:?}", e),
        }
    }
}

/// This is a share of a Beaver triple where `a * b = c`,
/// used for computing multiplication.
#[derive(Clone, Debug)]
pub struct TripleMsg {
    pub a: crypto::AuthShare,
    pub b: crypto::AuthShare,
    pub c: crypto::AuthShare,
}

impl TripleMsg {
    /// This function constructs a new triple message,
    /// the shares are assumed to be correct, i.e., `a*b = c`.
    pub fn new(a: crypto::AuthShare, b: crypto::AuthShare, c: crypto::AuthShare) -> TripleMsg {
        TripleMsg { a, b, c }
    }
}

/// This is a random sharing where only one party knows the random share,
/// used for inputting a secret value into the MPC.
#[derive(Clone, Debug)]
pub struct RandShareMsg {
    pub share: crypto::AuthShare,
    pub clear: Option<Fp>,
    pub party_id: PartyID,
}

#[derive(Clone, Debug)]
pub enum PreprocMsg {
    Triple(TripleMsg),
    RandShare(RandShareMsg),
}

impl PreprocMsg {
    pub fn new_triple(a: crypto::AuthShare, b: crypto::AuthShare, c: crypto::AuthShare) -> PreprocMsg {
        PreprocMsg::Triple(TripleMsg::new(a, b, c))
    }

    pub fn new_rand_share(share: crypto::AuthShare, clear: Option<Fp>, party_id: PartyID) -> PreprocMsg {
        PreprocMsg::RandShare(RandShareMsg { share, clear, party_id })
    }
}
