use crate::algebra::Fp;
use crate::message;
use crate::vm;
use crossbeam_channel;
use quick_error::quick_error;
use std::fmt;

#[derive(Debug)]
pub enum EvalError {
    OpenEmptyReg,
    OutputEmptyReg,
    OpEmptyReg,
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "evaluation failed with error {:?}", self)
    }
}

impl std::error::Error for EvalError {}

quick_error! {
    #[derive(Debug)]
    pub enum SomeError {
        EvalError(err: EvalError) {
            display("evaluation error: {}", err)
            from()
        }
        JoinError(err: Box<dyn std::any::Any + Send>) {
            display("join error: {:?}", err)
            from()
        }
        RecvError(err: crossbeam_channel::RecvError) {
            display("receive error: {}", err)
            from()
        }
        RecvTimeoutError(err: crossbeam_channel::RecvTimeoutError) {
            display("receive timeout error: {}", err)
            from()
        }
        SendErrorSyncMsg(err: crossbeam_channel::SendError<message::SyncMsg>) {
            display("send SyncMsgs error: {}", err)
            from()
        }
        SendErrorSyncMsgReply(err: crossbeam_channel::SendError<message::SyncMsgReply>) {
            display("send SyncMsgReply error: {}", err)
            from()
        }
        SendErrorAction(err: crossbeam_channel::SendError<vm::Action>) {
            display("send Action error: {}", err)
            from()
        }
        SendErrorInstruction(err: crossbeam_channel::SendError<vm::Instruction>) {
            display("send Instruction error: {}", err)
            from()
        }
        SendErrorTriple(err: crossbeam_channel::SendError<(Fp, Fp, Fp)>) {
            display("send Triple error: {}", err)
            from()
        }
        SendErrorFp(err: crossbeam_channel::SendError<Fp>) {
            display("send Fp error: {}", err)
            from()
        }
    }
}
