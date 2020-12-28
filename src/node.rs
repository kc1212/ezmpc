use crate::algebra::Fp;
use crate::crypto::commit;
use crate::crypto::AuthShare;
use crate::error::{OutputError, SomeError, TIMEOUT};
use crate::message::*;
use crate::vm;

use crossbeam_channel::{bounded, select, Receiver, Sender};
use log::debug;
use num_traits::Zero;
use rand::{SeedableRng, StdRng};
use std::thread;
use std::thread::JoinHandle;

pub struct Node {
    s_sync_chan: Sender<SyncMsgReply>,
    r_sync_chan: Receiver<SyncMsg>,
    triple_chan: Receiver<(AuthShare, AuthShare, AuthShare)>,
    rand_chan: Receiver<InputRandMsg>,
    s_node_chan: Vec<Sender<NodeMsg>>,
    r_node_chan: Vec<Receiver<NodeMsg>>,
    com_scheme: commit::Scheme,
}

impl Node {
    pub fn spawn(
        id: PartyID,
        alpha_share: Fp,
        reg: vm::Reg,
        instructions: Vec<vm::Instruction>,
        s_sync_chan: Sender<SyncMsgReply>,
        r_sync_chan: Receiver<SyncMsg>,
        triple_chan: Receiver<(AuthShare, AuthShare, AuthShare)>,
        rand_chan: Receiver<InputRandMsg>,
        s_node_chan: Vec<Sender<NodeMsg>>,
        r_node_chan: Vec<Receiver<NodeMsg>>,
        com_scheme: commit::Scheme,
        rng_seed: [usize; 4],
    ) -> JoinHandle<Result<Vec<Fp>, SomeError>> {
        thread::spawn(move || {
            let mut s = Node {
                s_sync_chan,
                r_sync_chan,
                triple_chan,
                rand_chan,
                s_node_chan,
                r_node_chan,
                com_scheme,
            };
            s.listen(id, alpha_share, reg, instructions, rng_seed)
        })
    }

    fn listen(&mut self, id: PartyID, alpha_share: Fp, reg: vm::Reg, prog: Vec<vm::Instruction>, rng_seed: [usize; 4]) -> Result<Vec<Fp>, SomeError> {
        let rng = &mut StdRng::from_seed(&rng_seed);

        // wait for start
        loop {
            let msg = self.r_sync_chan.recv()?;
            if msg == SyncMsg::Start {
                debug!("Starting");
                break;
            } else {
                debug!("Received {:?} while waiting to start", msg);
            }
        }

        // init forwarding channels
        let (s_inner_triple_chan, r_inner_triple_chan) = bounded(1024); // TODO what should the cap be?
        let (s_inner_rand_chan, r_inner_rand_chan) = bounded(1024); // TODO what should the cap be?

        // start the vm
        let (s_inst_chan, r_inst_chan) = bounded(5);
        let (s_action_chan, r_action_chan) = bounded(5);
        let vm_handler: JoinHandle<_> = vm::VM::spawn(id, alpha_share, reg, r_inner_triple_chan, r_inner_rand_chan, r_inst_chan, s_action_chan);
        let mut pc = 0;

        let unwrap_elem_msg = |msg: &NodeMsg| -> Fp {
            match msg {
                NodeMsg::Elem(x) => *x,
                e => panic!("expected an element message but got {:?}", e),
            }
        };

        let unwrap_com_msg = |msg: &NodeMsg| -> commit::Commitment {
            match msg {
                NodeMsg::Com(c) => *c,
                e => panic!("expected a com message but got {:?}", e),
            }
        };

        let unwrap_open_msg = |msg: &NodeMsg| -> commit::Opening {
            match msg {
                NodeMsg::Opening(o) => *o,
                e => panic!("expected an open message but got {:?}", e),
            }
        };

        let bcast = |m| broadcast(&self.s_node_chan, m);
        let recv = || recv_all(&self.r_node_chan, TIMEOUT);

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
                        SyncMsg::Start => panic!("node already started"),
                        SyncMsg::Next => {
                            if pc >= prog.len() {
                                panic!("instruction counter overflow");
                            }
                            let instruction = prog[pc];
                            pc += 1;
                            debug!("Sending instruction {:?} to VM", instruction);
                            s_inst_chan.send(instruction)?;

                            if instruction == vm::Instruction::Stop {
                                self.s_sync_chan.send(SyncMsgReply::Done)?;
                                break;
                            } else {
                                let action = r_action_chan.recv_timeout(TIMEOUT)?;
                                debug!("Received action {:?} from VM", action);
                                match action {
                                    vm::Action::None => (),
                                    vm::Action::Open(x, sender) => {
                                        // TODO add partially opened values to be checked by SOutput
                                        bcast(NodeMsg::Elem(x))?;
                                        let result = recv()?.iter().map(unwrap_elem_msg).sum();
                                        sender.send(result)?
                                    }
                                    vm::Action::Input(id, e_option, sender) => {
                                        match e_option {
                                            Some(e) => bcast(NodeMsg::Elem(e))?,
                                            None => (),
                                        };
                                        let e = unwrap_elem_msg(&self.r_node_chan[id].recv_timeout(TIMEOUT)?);
                                        sender.send(e)?
                                    }
                                    vm::Action::SOutput(share, sender) => {
                                        // TODO check all partially opened values
                                        // open x
                                        bcast(NodeMsg::Elem(share.share))?;
                                        let x: Fp = recv()?.iter().map(unwrap_elem_msg).sum();
                                        // let d = alpha_i * x - mac_i
                                        let d = alpha_share * x - share.mac;
                                        // commit d
                                        let (d_com, d_open) = self.com_scheme.commit(d, rng);
                                        bcast(NodeMsg::Com(d_com))?;
                                        // get commitment from others
                                        let d_coms: Vec<_> = recv()?.iter().map(unwrap_com_msg).collect();
                                        // commit-open d and collect them
                                        bcast(NodeMsg::Opening(d_open))?;
                                        let d_opens: Vec<_> = recv()?.iter().map(unwrap_open_msg).collect();
                                        // verify all the commitments of d
                                        // and check they sum to 0
                                        let coms_ok = d_opens.iter().zip(d_coms).map(|(o, c)| self.com_scheme.verify(&o, &c)).all(|x| x);
                                        let zero_ok = d_opens.into_iter().map(|o| o.get_v()).sum::<Fp>() == Fp::zero();
                                        if !coms_ok {
                                            sender.send(Err(OutputError::BadCommitment))?;
                                        } else if !zero_ok {
                                            sender.send(Err(OutputError::SumIsNotZero))?;
                                        } else {
                                            sender.send(Ok(()))?;
                                        }
                                    }
                                }
                                self.s_sync_chan.send(SyncMsgReply::Ok)?;
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

// TODO add node specific tests using a mock VM
