use crate::algebra::Fp;
use crate::crypto::AuthShare;
use crate::message;
use crate::vm;

use crossbeam_channel;
use std::fmt;
use std::time::Duration;
use thiserror::Error;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(1);

/// `OutputError` describes the different failure states when outputting a secret value.
#[derive(Debug)]
pub enum OutputError {
    RegisterEmpty,
    BadCommitment,
    SumIsNotZero,
}

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "output failed with error {:?}", self)
    }
}

impl std::error::Error for OutputError {}

/// `MPCError` is a wrapper for all the errors in this software to make error handling easier.
/// We do not use a generic parameter for the `SendError`s
/// so that functions that return `Result` also do not need a generic parameter,
/// making it a bit more user-friendly.
#[derive(Error, Debug)]
pub enum MPCError {
    #[error("empty register")]
    EmptyError,
    #[error(transparent)]
    OutputError(#[from] OutputError),
    #[error(transparent)]
    RecvError(#[from] crossbeam_channel::RecvError),
    #[error(transparent)]
    RecvTimeoutError(#[from] crossbeam_channel::RecvTimeoutError),
    #[error(transparent)]
    SendErrorSyncMsg(#[from] crossbeam_channel::SendError<message::SyncMsg>),
    #[error(transparent)]
    SendErrorSyncMsgReply(#[from] crossbeam_channel::SendError<message::SyncMsgReply>),
    #[error(transparent)]
    SendErrorNodeMsg(#[from] crossbeam_channel::SendError<message::NodeMsg>),
    #[error(transparent)]
    SendErrorInputRandMsg(#[from] crossbeam_channel::SendError<message::InputRandMsg>),
    #[error(transparent)]
    SendErrorAction(#[from] crossbeam_channel::SendError<vm::Action>),
    #[error(transparent)]
    SendErrorInstruction(#[from] crossbeam_channel::SendError<vm::Instruction>),
    #[error(transparent)]
    SendErrorFp(#[from] crossbeam_channel::SendError<Fp>),
    #[error(transparent)]
    SendErrorOutputResult(#[from] crossbeam_channel::SendError<Result<(), OutputError>>),
    #[error(transparent)]
    TrySendErrorTriple(#[from] crossbeam_channel::TrySendError<(AuthShare, AuthShare, AuthShare)>),
    #[error(transparent)]
    TrySendErrorInputRandMsg(#[from] crossbeam_channel::TrySendError<message::InputRandMsg>),
}
