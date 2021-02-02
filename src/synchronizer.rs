//! This module contains a simple implementation of an alpha-synchronizer
//! that communicates using channels.

use crate::error::{MPCError, TIMEOUT};
use crate::message;
use crate::message::{SyncMsg, SyncReplyMsg};

use crossbeam::channel::{Receiver, RecvTimeoutError, SendError, Sender};
use log::debug;
use std::thread;

pub struct Synchronizer {
    s_chans: Vec<Sender<SyncMsg>>,
    r_chans: Vec<Receiver<SyncReplyMsg>>,
}

impl Synchronizer {
    /// Spawn a thread that runs the synchronizer.
    /// It reads messages from `r_chans` and sends messages using `s_chans`.
    /// These channels are assumed to be correctly connected to the parties.
    pub fn spawn(s_chans: Vec<Sender<SyncMsg>>, r_chans: Vec<Receiver<SyncReplyMsg>>) -> thread::JoinHandle<Result<(), MPCError>> {
        thread::spawn(move || {
            let s = Synchronizer { s_chans, r_chans };
            s.broadcast(SyncMsg::Start)?;
            debug!("Starting");
            s.listen()
        })
    }

    fn broadcast(&self, m: SyncMsg) -> Result<(), SendError<SyncMsg>> {
        message::broadcast(&self.s_chans, m)
    }

    fn recv_all(&self) -> Result<Vec<SyncReplyMsg>, RecvTimeoutError> {
        message::receive(&self.r_chans, TIMEOUT)
    }

    fn listen(&self) -> Result<(), MPCError> {
        self.broadcast(SyncMsg::Next)?;
        loop {
            let msgs = self.recv_all()?;
            if msgs.iter().all(|x| *x == SyncReplyMsg::Done) {
                debug!("All done");
                break;
            } else if msgs.contains(&SyncReplyMsg::Abort) {
                self.broadcast(SyncMsg::Abort)?;
                break;
            } else if msgs.iter().all(|x| *x == SyncReplyMsg::Ok) {
                self.broadcast(SyncMsg::Next)?;
            } else {
                panic!("unexpected messages {:?}", msgs);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{SyncMsg, SyncReplyMsg};
    use crossbeam::channel::bounded;

    const TEST_CAP: usize = 5;

    #[test]
    fn test_synchronizer() {
        let (s_msg, r_msg) = bounded(TEST_CAP);
        let (s_reply, r_reply) = bounded(TEST_CAP);
        let handler = Synchronizer::spawn(vec![s_msg], vec![r_reply]);

        // we expect to hear a Start followed by a Next
        assert_eq!(SyncMsg::Start, r_msg.recv_timeout(TIMEOUT).unwrap());
        assert_eq!(SyncMsg::Next, r_msg.recv_timeout(TIMEOUT).unwrap());

        // then we expect Next again after sending Ok
        s_reply.send(SyncReplyMsg::Ok).unwrap();
        assert_eq!(SyncMsg::Next, r_msg.recv_timeout(TIMEOUT).unwrap());

        // finally, sending Abort will respond with Abort
        s_reply.send(SyncReplyMsg::Abort).unwrap();
        assert_eq!(SyncMsg::Abort, r_msg.recv_timeout(TIMEOUT).unwrap());

        assert_eq!((), handler.join().unwrap().unwrap());
    }
}
