//! This module implement a party that participates in the MPC protocol.
//! It assumes perfect channels for sending and receiving messages.
//! The actual networking layer is handled by an outer layer.

use crate::algebra::Fp;
use crate::crypto::commit;
use crate::crypto::AuthShare;
use crate::error::{MACCheckError, MPCError, TIMEOUT};
use crate::message;
use crate::message::{PartyID, PartyMsg, RandShareMsg, SyncMsg, SyncReplyMsg, TripleMsg};
use crate::vm;

use crossbeam_channel::{bounded, select, Receiver, Sender};
use log::{debug, error};
use num_traits::Zero;
use rand::{Rng, SeedableRng, StdRng};
use std::thread;
use std::thread::JoinHandle;

pub struct Party {
    id: PartyID,
    alpha_share: Fp,
    com_scheme: commit::Scheme,
    s_sync_chan: Sender<SyncReplyMsg>,
    r_sync_chan: Receiver<SyncMsg>,
    triple_chan: Receiver<TripleMsg>,
    rand_chan: Receiver<RandShareMsg>,
    s_party_chan: Vec<Sender<PartyMsg>>,
    r_party_chan: Vec<Receiver<PartyMsg>>,
}

impl Party {
    /// Spawn a party thread and returns a handler.
    /// If successful, the handler will return the result of the computation,
    /// i.e., the result of calling `COutput` or `SOutput`.
    pub fn spawn(
        id: PartyID,
        alpha_share: Fp,
        reg: vm::Reg,
        instructions: Vec<vm::Instruction>,
        s_sync_chan: Sender<SyncReplyMsg>,
        r_sync_chan: Receiver<SyncMsg>,
        triple_chan: Receiver<TripleMsg>,
        rand_chan: Receiver<RandShareMsg>,
        s_party_chan: Vec<Sender<PartyMsg>>,
        r_party_chan: Vec<Receiver<PartyMsg>>,
        rng_seed: [usize; 4],
    ) -> JoinHandle<Result<Vec<Fp>, MPCError>> {
        thread::spawn(move || {
            let s = Party {
                id,
                alpha_share,
                com_scheme: commit::Scheme {},
                s_sync_chan,
                r_sync_chan,
                triple_chan,
                rand_chan,
                s_party_chan,
                r_party_chan,
            };
            s.listen(reg, instructions, rng_seed)
        })
    }

    fn listen(&self, reg: vm::Reg, prog: Vec<vm::Instruction>, rng_seed: [usize; 4]) -> Result<Vec<Fp>, MPCError> {
        let rng = &mut StdRng::from_seed(&rng_seed);

        // init forwarding channels
        let (s_inner_triple_chan, r_inner_triple_chan) = bounded(1024); // TODO what should the cap be?
        let (s_inner_rand_chan, r_inner_rand_chan) = bounded(1024); // TODO what should the cap be?

        // start the vm
        let (s_inst_chan, r_inst_chan) = bounded(5);
        let (s_action_chan, r_action_chan) = bounded(5);
        let vm_handler: JoinHandle<_> = vm::VM::spawn(
            self.id,
            self.alpha_share,
            reg,
            r_inner_triple_chan,
            r_inner_rand_chan,
            r_inst_chan,
            s_action_chan,
        );
        let mut pc = 0;

        // wait for start
        loop {
            let msg = self.r_sync_chan.recv()?;
            if msg == SyncMsg::Start {
                debug!("[{}] Starting", self.id);
                break;
            } else {
                debug!("[{}] Received {:?} while waiting to start", self.id, msg);
            }
        }

        // process instructions
        loop {
            select! {
                recv(self.triple_chan) -> x => {
                    s_inner_triple_chan.try_send(x?)?
                }
                recv(self.rand_chan) -> x => {
                    s_inner_rand_chan.try_send(x?)?
                }
                recv(self.r_sync_chan) -> v => {
                    let msg: SyncMsg = v?;
                    match msg {
                        SyncMsg::Start => panic!("party already started"),
                        SyncMsg::Next => {
                            if pc >= prog.len() {
                                panic!("instruction counter overflow");
                            }
                            let instruction = prog[pc];
                            pc += 1;

                            debug!("[{}] Sending instruction {:?} to VM", self.id, instruction);
                            s_inst_chan.send(instruction)?;
                            self.handle_vm_actions(&r_action_chan, rng)?;

                            if instruction == vm::Instruction::Stop {
                                self.s_sync_chan.send(SyncReplyMsg::Done)?;
                                break;
                            } else {
                                self.s_sync_chan.send(SyncReplyMsg::Ok)?;
                            }
                        },
                        SyncMsg::Abort => panic!("abort"),
                    }
                }
            }
        }

        vm_handler.join().expect("thread panicked")
    }

    fn bcast(&self, m: PartyMsg) -> Result<(), MPCError> {
        message::broadcast(&self.s_party_chan, m)?;
        Ok(())
    }

    fn recv(&self) -> Result<Vec<PartyMsg>, MPCError> {
        let out = message::receive(&self.r_party_chan, TIMEOUT)?;
        Ok(out)
    }

    fn mac_check(&self, x: &Fp, share: &AuthShare, rng: &mut impl Rng) -> Result<Result<(), MACCheckError>, MPCError> {
        // let d = alpha_i * x - mac_i
        let d = self.alpha_share * x - share.mac;
        // commit d
        let (d_com, d_open) = self.com_scheme.commit(d, rng);
        self.bcast(PartyMsg::Com(d_com))?;
        // get commitment from others
        let d_coms: Vec<_> = self.recv()?.iter().map(|x| x.unwrap_com()).collect();
        // commit-open d and collect them
        self.bcast(PartyMsg::Opening(d_open))?;
        let d_opens: Vec<_> = self.recv()?.iter().map(|x| x.unwrap_opening()).collect();
        // verify all the commitments of d
        // and check they sum to 0
        let coms_ok = d_opens.iter().zip(d_coms).map(|(o, c)| self.com_scheme.verify(&o, &c)).all(|x| x);
        let zero_ok = d_opens.into_iter().map(|o| o.get_v()).sum::<Fp>() == Fp::zero();

        // this is a weird kind of return type but it makes categorizing the errors easier
        if !coms_ok {
            Ok(Err(MACCheckError::BadCommitment))
        } else if !zero_ok {
            Ok(Err(MACCheckError::SumIsNotZero))
        } else {
            Ok(Ok(()))
        }
    }

    fn handle_vm_actions(&self, r_action_chan: &Receiver<vm::Action>, rng: &mut impl Rng) -> Result<(), MPCError> {
        loop {
            let action = r_action_chan.recv_timeout(TIMEOUT)?;
            debug!("[{}], Received action {:?} from VM", self.id, action);
            match action {
                vm::Action::Next => {
                    break;
                }
                vm::Action::Open(x, sender) => {
                    self.bcast(PartyMsg::Elem(x))?;
                    let result = self.recv()?.iter().map(|x| x.unwrap_elem()).sum();
                    debug!("[{}] Partially opened {:?}", self.id, result);
                    sender.send(result)?
                }
                vm::Action::Input(id, e_option, sender) => {
                    match e_option {
                        Some(e) => self.bcast(PartyMsg::Elem(e))?,
                        None => (),
                    };
                    let e = self.r_party_chan[id].recv_timeout(TIMEOUT)?.unwrap_elem();
                    sender.send(e)?
                }
                vm::Action::Check(openings, sender) => {
                    // mac_check everything and send error on first failure
                    let mut ok = true;
                    for (x, opening) in openings {
                        match self.mac_check(&x, &opening, rng)? {
                            Ok(()) => {}
                            e => {
                                error!("[{}] MAC check failed: {:?}", self.id, e);
                                sender.send(e)?;
                                ok = false;
                                break;
                            }
                        }
                    }

                    if ok {
                        debug!("[{}] All MAC check ok", self.id);
                        sender.send(Ok(()))?;
                    }
                }
            }
        }
        Ok(())
    }
}

// TODO add party specific tests using a mock VM
