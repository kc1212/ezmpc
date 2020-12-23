use crate::message::{SyncMsg, SyncMsgReply};
use crossbeam_channel;

quick_error! {
    #[derive(Debug, Eq, PartialEq)]
    pub enum SomeError {
        EvalError {
            display("evaluation error")
        }
        RecvError(err: crossbeam_channel::RecvError) {
            from()
        }
        RecvTimeoutError(err: crossbeam_channel::RecvTimeoutError) {
            from()
        }
        SendError(err: crossbeam_channel::SendError<SyncMsg>) {
            from()
        }
        SendErrorReply(err: crossbeam_channel::SendError<SyncMsgReply>) {
            from()
        }
    }
}
