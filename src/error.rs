use crate::message::{SyncMsg, SyncMsgReply};
use crate::vm;
use crossbeam_channel;

quick_error! {
    #[derive(Debug)]
    pub enum SomeError {
        EvalError {
            display("evaluation error")
        }
        NoneError {
            display("none error")
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
        SendErrorAction(err: crossbeam_channel::SendError<vm::Action>) {
            from()
        }
        SendErrorInstruction(err: crossbeam_channel::SendError<vm::Instruction>) {
            from()
        }
    }
}
