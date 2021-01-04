//! This module defines error types that are used in this crate.

use crate::algebra::Fp;
use crate::message;
use crate::vm;

use crossbeam_channel;
use std::fmt;
use std::time::Duration;
use thiserror::Error;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(1);

/// `MACCheckError` describes the different failure states when checking a MAC.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MACCheckError {
    BadCommitment,
    SumIsNotZero,
}

impl fmt::Display for MACCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "output failed with error {:?}", self)
    }
}

impl std::error::Error for MACCheckError {}

/// `MPCError` is a wrapper for all the errors in this software to make error handling easier.
/// We do not use a generic parameter for the `SendError`s
/// so that functions that return `Result` also do not need a generic parameter,
/// making it a bit more user-friendly.
#[derive(Error, Debug)]
pub enum MPCError {
    #[error("empty register")]
    EmptyError,
    #[error(transparent)]
    MACCheckError(#[from] MACCheckError),
    #[error(transparent)]
    RecvError(#[from] crossbeam_channel::RecvError),
    #[error(transparent)]
    RecvTimeoutError(#[from] crossbeam_channel::RecvTimeoutError),
    #[error(transparent)]
    SendErrorSyncMsg(#[from] crossbeam_channel::SendError<message::SyncMsg>),
    #[error(transparent)]
    SendErrorSyncReplyMsg(#[from] crossbeam_channel::SendError<message::SyncReplyMsg>),
    #[error(transparent)]
    SendErrorNodeMsg(#[from] crossbeam_channel::SendError<message::PartyMsg>),
    #[error(transparent)]
    SendErrorInputRandMsg(#[from] crossbeam_channel::SendError<message::RandShareMsg>),
    #[error(transparent)]
    SendErrorAction(#[from] crossbeam_channel::SendError<vm::Action>),
    #[error(transparent)]
    SendErrorInstruction(#[from] crossbeam_channel::SendError<vm::Instruction>),
    #[error(transparent)]
    SendErrorFp(#[from] crossbeam_channel::SendError<Fp>),
    #[error(transparent)]
    SendErrorOutputResult(#[from] crossbeam_channel::SendError<Result<(), MACCheckError>>),
    #[error(transparent)]
    TrySendErrorTriple(#[from] crossbeam_channel::TrySendError<message::TripleMsg>),
    #[error(transparent)]
    TrySendErrorRandShareMsg(#[from] crossbeam_channel::TrySendError<message::RandShareMsg>),
}
