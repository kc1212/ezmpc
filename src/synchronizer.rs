use crate::error::SomeError;
use crate::message::*;
use crossbeam_channel::{Receiver, RecvTimeoutError, SendError, Sender};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

pub struct Synchronizer {
    o_chans: Vec<Sender<SyncMsg>>,
    i_chans: Vec<Receiver<SyncMsgReply>>,
}

impl Synchronizer {
    pub fn spawn(
        o_chans: Vec<Sender<SyncMsg>>,
        i_chans: Vec<Receiver<SyncMsgReply>>,
    ) -> JoinHandle<Result<(), SomeError>> {
        thread::spawn(move || {
            let s = Synchronizer { o_chans, i_chans };
            s.broadcast(SyncMsg::Start)?;
            println!("start!");
            s.listen()
        })
    }

    fn broadcast(&self, m: SyncMsg) -> Result<(), SendError<SyncMsg>> {
        for c in &self.o_chans {
            println!("sync sending {:?}", m);
            c.send(m)?;
        }
        Ok(())
    }

    fn recv_all(&self) -> Result<Vec<SyncMsgReply>, RecvTimeoutError> {
        let mut out: Vec<SyncMsgReply> = Vec::new();
        for c in &self.i_chans {
            let m = c.recv_timeout(Duration::from_millis(1000))?;
            out.push(m);
        }
        Ok(out)
    }

    fn listen(&self) -> Result<(), SomeError> {
        self.broadcast(SyncMsg::Next)?;
        loop {
            let msgs = self.recv_all()?;
            if msgs.iter().all(|x| *x == SyncMsgReply::Done) {
                break;
            } else if msgs.contains(&SyncMsgReply::Abort) {
                self.broadcast(SyncMsg::Abort)?;
                break;
            } else if msgs.iter().all(|x| *x == SyncMsgReply::Ok) {
                self.broadcast(SyncMsg::Next)?;
            } else {
                panic!("unexpected condition");
            }
        }
        Ok(())
    }
}
