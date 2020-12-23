use crate::crypto::Fp;
use crate::error::SomeError;
use crate::message::*;
use crate::vm;
use crossbeam_channel::{select, Receiver, Sender, bounded};
use log::debug;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

pub struct Machine {
    o_chans: Sender<SyncMsgReply>,
    i_chans: Receiver<SyncMsg>,
    triple_chan: Receiver<TripleMsg>,
    instructions: Vec<vm::Instruction>,
}

impl Machine {
    pub fn spawn(
        id: vm::PartyID,
        o_chans: Sender<SyncMsgReply>,
        i_chans: Receiver<SyncMsg>,
        triple_chan: Receiver<TripleMsg>,
        instructions: Vec<vm::Instruction>,
        reg: vm::Reg,
    ) -> JoinHandle<Result<Vec<Fp>, SomeError>> {
        thread::spawn(move || {
            let mut s = Machine {
                o_chans,
                i_chans,
                triple_chan,
                instructions,
            };
            s.listen(id, reg)
        })
    }

    fn listen(&mut self, id: vm::PartyID, reg: vm::Reg) -> Result<Vec<Fp>, SomeError> {
        // wait for start
        loop {
            let msg = self.i_chans.recv()?;
            if msg == SyncMsg::Start {
                debug!("Machine is starting");
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

        // process instructions
        loop {
            select! {
                recv(self.triple_chan) -> _ => () /* TODO */,
                recv(self.i_chans) -> v => {
                    let msg: SyncMsg = v?;
                    match msg {
                        SyncMsg::Start => panic!("already started"),
                        SyncMsg::Next => {
                            if instruction_counter >= self.instructions.len() {
                                panic!("instruction counter overflow");
                            }
                            let inst = self.instructions[instruction_counter];
                            instruction_counter += 1;
                            debug!("Sending inst {:?} to vm", inst);
                            s_inst_chan.send(inst)?;

                            if inst == vm::Instruction::STOP {
                                self.o_chans.send(SyncMsgReply::Done)?;
                                break;
                            } else {
                                let action = r_action_chan.recv_timeout(Duration::from_secs(1))?;
                                debug!("Received action {:?} from vm", action);
                                match action {
                                    vm::Action::None => (),
                                    vm::Action::Open(_, sender) => {
                                        unimplemented!()
                                    }
                                    vm::Action::Triple(sender) => {
                                        unimplemented!()
                                    }
                                }
                                self.o_chans.send(SyncMsgReply::Ok)?;
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

