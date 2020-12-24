use crate::crypto::Fp;
use crate::error::SomeError;
use crate::message::*;
use crate::vm;
use crossbeam_channel::{select, Receiver, Sender, bounded};
use ff::Field;
use log::debug;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

pub struct Node {
    s_sync_chan: Sender<SyncMsgReply>,
    r_sync_chan: Receiver<SyncMsg>,
    triple_chan: Receiver<(Fp, Fp, Fp)>,
    s_node_chan: Vec<Sender<Fp>>,
    r_node_chan: Vec<Receiver<Fp>>,
    instructions: Vec<vm::Instruction>,
}

impl Node {
    pub fn spawn(
        id: vm::PartyID,
        s_sync_chan: Sender<SyncMsgReply>,
        r_sync_chan: Receiver<SyncMsg>,
        triple_chan: Receiver<(Fp, Fp, Fp)>,
        s_node_chan: Vec<Sender<Fp>>,
        r_node_chan: Vec<Receiver<Fp>>,
        instructions: Vec<vm::Instruction>,
        reg: vm::Reg,
    ) -> JoinHandle<Result<Vec<Fp>, SomeError>> {
        thread::spawn(move || {
            let mut s = Node {
                s_sync_chan,
                r_sync_chan,
                triple_chan,
                s_node_chan,
                r_node_chan,
                instructions,
            };
            s.listen(id, reg)
        })
    }

    fn listen(&mut self, id: vm::PartyID, reg: vm::Reg) -> Result<Vec<Fp>, SomeError> {
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

        // start the vm
        let (s_inst_chan, r_inst_chan) = bounded(5);
        let (s_action_chan, r_action_chan) = bounded(5);
        let vm_handler = vm::VM::spawn(id, reg, r_inst_chan, s_action_chan);
        let mut instruction_counter = 0;
        let mut triples = Vec::new();

        // process instructions
        loop {
            select! {
                recv(self.triple_chan) -> triple => {
                    match triple {
                        Ok(t) => triples.push(t),
                        Err(e) => debug!("Triple error {}", e),
                    }
                }
                recv(self.r_sync_chan) -> v => {
                    let msg: SyncMsg = v?;
                    match msg {
                        SyncMsg::Start => panic!("node already started"),
                        SyncMsg::Next => {
                            if instruction_counter >= self.instructions.len() {
                                panic!("instruction counter overflow");
                            }
                            let inst = self.instructions[instruction_counter];
                            instruction_counter += 1;
                            debug!("Sending instruction {:?} to VM", inst);
                            s_inst_chan.send(inst)?;

                            if inst == vm::Instruction::STOP {
                                self.s_sync_chan.send(SyncMsgReply::Done)?;
                                break;
                            } else {
                                let action = r_action_chan.recv_timeout(Duration::from_secs(1))?;
                                debug!("Received action {:?} from VM", action);
                                match action {
                                    vm::Action::None => (),
                                    vm::Action::Open(x, sender) => {
                                        broadcast(&self.s_node_chan, x)?;
                                        let replies = recv_all(&self.r_node_chan)?;
                                        let mut result = Fp::zero();
                                        for reply in replies {
                                            result.add_assign(&reply);
                                        }
                                        sender.send(result)?
                                    }
                                    vm::Action::Triple(sender) => {
                                        let triple: (Fp, Fp, Fp) = match triples.pop() {
                                            Some(t) => t,
                                            None => self.triple_chan.recv_timeout(Duration::from_secs(1))?,
                                        };
                                        sender.send(triple)?
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

        match vm_handler.join() {
            Ok(x) => x,
            Err(e) => Err(SomeError::JoinError),
        }
    }
}

