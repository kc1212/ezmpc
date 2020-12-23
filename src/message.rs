use crate::crypto::Fp;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SyncMsg {
    Start,
    Next,
    Abort,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SyncMsgReply {
    Ok,
    Done,
    Abort,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TripleMsg {
    a: Fp,
    b: Fp,
    c: Fp,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Inst {
    CAdd,
    CMul,
    SAdd,
    Triple,
    Open,
    CPush(Fp),
    SPush(Fp),
    COutput,
    SOutput,
}


