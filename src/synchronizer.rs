use crate::error::{SomeError, TIMEOUT};
use crate::message::*;
use crossbeam_channel::{Receiver, RecvTimeoutError, SendError, Sender};
use log::debug;
use std::thread;
use std::thread::JoinHandle;

pub struct Synchronizer {
    s_chans: Vec<Sender<SyncMsg>>,
    r_chans: Vec<Receiver<SyncMsgReply>>,
}

impl Synchronizer {
    pub fn spawn(s_chans: Vec<Sender<SyncMsg>>, r_chans: Vec<Receiver<SyncMsgReply>>) -> JoinHandle<Result<(), SomeError>> {
        thread::spawn(move || {
            let s = Synchronizer { s_chans, r_chans };
            s.broadcast(SyncMsg::Start)?;
            debug!("Starting");
            s.listen()
        })
    }

    fn broadcast(&self, m: SyncMsg) -> Result<(), SendError<SyncMsg>> {
        broadcast(&self.s_chans, m)
    }

    fn recv_all(&self) -> Result<Vec<SyncMsgReply>, RecvTimeoutError> {
        recv_all(&self.r_chans, TIMEOUT)
    }

    fn listen(&self) -> Result<(), SomeError> {
        self.broadcast(SyncMsg::Next)?;
        loop {
            let msgs = self.recv_all()?;
            if msgs.iter().all(|x| *x == SyncMsgReply::Done) {
                debug!("All done");
                break;
            } else if msgs.contains(&SyncMsgReply::Abort) {
                self.broadcast(SyncMsg::Abort)?;
                break;
            } else if msgs.iter().all(|x| *x == SyncMsgReply::Ok) {
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
    use crate::message::{SyncMsg, SyncMsgReply};
    use crossbeam_channel::bounded;

    #[test]
    fn test_synchronizer() {
        let (s_msg, r_msg) = bounded(5);
        let (s_reply, r_reply) = bounded(5);
        let handler = Synchronizer::spawn(vec![s_msg], vec![r_reply]);

        // we expect to hear a Start followed by a Next
        assert_eq!(SyncMsg::Start, r_msg.recv_timeout(TIMEOUT).unwrap());
        assert_eq!(SyncMsg::Next, r_msg.recv_timeout(TIMEOUT).unwrap());

        // then we expect Next again after sending Ok
        s_reply.send(SyncMsgReply::Ok).unwrap();
        assert_eq!(SyncMsg::Next, r_msg.recv_timeout(TIMEOUT).unwrap());

        // finally, sending Abort will respond with Abort
        s_reply.send(SyncMsgReply::Abort).unwrap();
        assert_eq!(SyncMsg::Abort, r_msg.recv_timeout(TIMEOUT).unwrap());

        assert_eq!((), handler.join().unwrap().unwrap());
    }
}
