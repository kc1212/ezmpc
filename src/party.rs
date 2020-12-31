use crate::algebra::Fp;
use crate::crypto::commit;
use crate::crypto::AuthShare;
use crate::error::{MPCError, OutputError, TIMEOUT};
use crate::message::*;
use crate::vm;

use crossbeam_channel::{bounded, select, Receiver, Sender};
use log::{debug, error};
use num_traits::Zero;
use rand::{SeedableRng, StdRng};
use std::thread;
use std::thread::JoinHandle;

pub struct Party {
    s_sync_chan: Sender<SyncReplyMsg>,
    r_sync_chan: Receiver<SyncMsg>,
    triple_chan: Receiver<TripleMsg>,
    rand_chan: Receiver<RandShareMsg>,
    s_party_chan: Vec<Sender<PartyMsg>>,
    r_party_chan: Vec<Receiver<PartyMsg>>,
    com_scheme: commit::Scheme,
}

impl Party {
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
        com_scheme: commit::Scheme,
        rng_seed: [usize; 4],
    ) -> JoinHandle<Result<Vec<Fp>, MPCError>> {
        thread::spawn(move || {
            let mut s = Party {
                s_sync_chan,
                r_sync_chan,
                triple_chan,
                rand_chan,
                s_party_chan,
                r_party_chan,
                com_scheme,
            };
            s.listen(id, alpha_share, reg, instructions, rng_seed)
        })
    }

    fn listen(&mut self, id: PartyID, alpha_share: Fp, reg: vm::Reg, prog: Vec<vm::Instruction>, rng_seed: [usize; 4]) -> Result<Vec<Fp>, MPCError> {
        let rng = &mut StdRng::from_seed(&rng_seed);

        // init forwarding channels
        let (s_inner_triple_chan, r_inner_triple_chan) = bounded(1024); // TODO what should the cap be?
        let (s_inner_rand_chan, r_inner_rand_chan) = bounded(1024); // TODO what should the cap be?

        // start the vm
        let (s_inst_chan, r_inst_chan) = bounded(5);
        let (s_action_chan, r_action_chan) = bounded(5);
        let vm_handler: JoinHandle<_> = vm::VM::spawn(id, alpha_share, reg, r_inner_triple_chan, r_inner_rand_chan, r_inst_chan, s_action_chan);
        let mut pc = 0;

        let unwrap_elem_msg = |msg: &PartyMsg| -> Fp {
            match msg {
                PartyMsg::Elem(x) => *x,
                e => panic!("expected an element message but got {:?}", e),
            }
        };

        let unwrap_com_msg = |msg: &PartyMsg| -> commit::Commitment {
            match msg {
                PartyMsg::Com(c) => *c,
                e => panic!("expected a com message but got {:?}", e),
            }
        };

        let unwrap_open_msg = |msg: &PartyMsg| -> commit::Opening {
            match msg {
                PartyMsg::Opening(o) => *o,
                e => panic!("expected an open message but got {:?}", e),
            }
        };

        let bcast = |m| broadcast(&self.s_party_chan, m);
        let recv = || recv_all(&self.r_party_chan, TIMEOUT);

        // perform one MAC check
        let mut mac_check = |x: &Fp, share: &AuthShare| -> Result<Result<(), OutputError>, MPCError> {
            // let d = alpha_i * x - mac_i
            let d = alpha_share * x - share.mac;
            // commit d
            let (d_com, d_open) = self.com_scheme.commit(d, rng);
            bcast(PartyMsg::Com(d_com))?;
            // get commitment from others
            let d_coms: Vec<_> = recv()?.iter().map(unwrap_com_msg).collect();
            // commit-open d and collect them
            bcast(PartyMsg::Opening(d_open))?;
            let d_opens: Vec<_> = recv()?.iter().map(unwrap_open_msg).collect();
            // verify all the commitments of d
            // and check they sum to 0
            let coms_ok = d_opens.iter().zip(d_coms).map(|(o, c)| self.com_scheme.verify(&o, &c)).all(|x| x);
            let zero_ok = d_opens.into_iter().map(|o| o.get_v()).sum::<Fp>() == Fp::zero();

            // this is a weird kind of type but it makes categorizing the errors easier
            if !coms_ok {
                Ok(Err(OutputError::BadCommitment))
            } else if !zero_ok {
                Ok(Err(OutputError::SumIsNotZero))
            } else {
                Ok(Ok(()))
            }
        };

        // handle action items from the VM
        let mut handle_action = || -> Result<(), MPCError> {
            loop {
                let action = r_action_chan.recv_timeout(TIMEOUT)?;
                debug!("[{}], Received action {:?} from VM", id, action);
                match action {
                    vm::Action::Next => {
                        break;
                    }
                    vm::Action::Open(x, sender) => {
                        bcast(PartyMsg::Elem(x))?;
                        let result = recv()?.iter().map(unwrap_elem_msg).sum();
                        debug!("[{}] Partially opened {:?}", id, result);
                        sender.send(result)?
                    }
                    vm::Action::Input(id, e_option, sender) => {
                        match e_option {
                            Some(e) => bcast(PartyMsg::Elem(e))?,
                            None => (),
                        };
                        let e = unwrap_elem_msg(&self.r_party_chan[id].recv_timeout(TIMEOUT)?);
                        sender.send(e)?
                    }
                    vm::Action::Check(openings, sender) => {
                        // mac_check everything and send error on first failure
                        let mut ok = true;
                        for (x, opening) in openings {
                            match mac_check(&x, &opening)? {
                                Ok(()) => {}
                                e => {
                                    error!("[{}] MAC check failed: {:?}", id, e);
                                    sender.send(e)?;
                                    ok = false;
                                    break;
                                }
                            }
                        }

                        if ok {
                            debug!("[{}] All MAC check ok", id);
                            sender.send(Ok(()))?;
                        }
                    }
                }
            }
            Ok(())
        };

        // wait for start
        loop {
            let msg = self.r_sync_chan.recv()?;
            if msg == SyncMsg::Start {
                debug!("[{}] Starting", id);
                break;
            } else {
                debug!("[{}] Received {:?} while waiting to start", id, msg);
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

                            debug!("[{}] Sending instruction {:?} to VM", id, instruction);
                            s_inst_chan.send(instruction)?;
                            handle_action()?;

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
}

// TODO add party specific tests using a mock VM
