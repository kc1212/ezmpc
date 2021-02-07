//! This module defines error types that are used in this crate.

use crate::algebra::Fp;
use crate::message;
use crate::vm;

use bincode;
use crossbeam::channel;
use ron;
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
    RecvError(#[from] channel::RecvError),
    #[error(transparent)]
    RecvTimeoutError(#[from] channel::RecvTimeoutError),
    #[error(transparent)]
    SendErrorSyncMsg(#[from] channel::SendError<message::SyncMsg>),
    #[error(transparent)]
    SendErrorSyncReplyMsg(#[from] channel::SendError<message::SyncReplyMsg>),
    #[error(transparent)]
    SendErrorNodeMsg(#[from] channel::SendError<message::PartyMsg>),
    #[error(transparent)]
    SendErrorInputRandMsg(#[from] channel::SendError<message::RandShareMsg>),
    #[error(transparent)]
    SendErrorAction(#[from] channel::SendError<vm::Action>),
    #[error(transparent)]
    SendErrorInstruction(#[from] channel::SendError<vm::Instruction>),
    #[error(transparent)]
    SendErrorFp(#[from] channel::SendError<Fp>),
    #[error(transparent)]
    SendErrorOutputResult(#[from] channel::SendError<Result<(), MACCheckError>>),
    #[error(transparent)]
    TrySendErrorTriple(#[from] channel::TrySendError<message::TripleMsg>),
    #[error(transparent)]
    TrySendErrorRandShareMsg(#[from] channel::TrySendError<message::RandShareMsg>),
}

#[derive(Error, Debug)]
pub enum ApplicationError {
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    RonError(#[from] ron::Error),
    #[error(transparent)]
    BincodeError(#[from] Box<bincode::ErrorKind>),
    #[error(transparent)]
    MPCError(#[from] MPCError),
}
