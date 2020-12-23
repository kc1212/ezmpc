use crate::crypto::Fp;
use crate::error::SomeError;
use crate::message::*;
use crossbeam_channel::{select, Receiver, Sender};
use ff::Field;
use std::thread;
use std::thread::JoinHandle;

pub struct Machine {
    o_chans: Sender<SyncMsgReply>,
    i_chans: Receiver<SyncMsg>,
    triple_chan: Receiver<TripleMsg>,
    instructions: Vec<Inst>,
    c_stack: Vec<Fp>,
    s_stack: Vec<Fp>,
    c_output: Vec<Fp>,
    s_output: Vec<Fp>,
}

fn eval(
    inst: &Inst,
    c_stack: &mut Vec<Fp>,
    s_stack: &mut Vec<Fp>,
    c_output: &mut Vec<Fp>,
    s_output: &mut Vec<Fp>,
) -> Option<()> {
    match inst {
        Inst::CAdd => {
            let mut a = c_stack.pop()?;
            let b = c_stack.pop()?;

            a.add_assign(&b);
            c_stack.push(a);
        }
        Inst::CMul => {
            let mut a = c_stack.pop().unwrap();
            let b = c_stack.pop().unwrap();

            a.mul_assign(&b);
            c_stack.push(a);
        }
        Inst::SAdd => {
            unimplemented!()
        }
        Inst::Triple => {
            unimplemented!()
        }
        Inst::Open => {
            unimplemented!()
        }
        Inst::CPush(e) => {
            c_stack.push(e.clone());
        }
        Inst::SPush(e) => {
            s_stack.push(e.clone());
        }
        Inst::COutput => {
            c_output.push(c_stack.last()?.clone());
        }
        Inst::SOutput => {
            s_output.push(s_stack.last()?.clone());
        }
    };
    Some(())
}

impl Machine {
    pub fn spawn(
        o_chans: Sender<SyncMsgReply>,
        i_chans: Receiver<SyncMsg>,
        triple_chan: Receiver<TripleMsg>,
        instructions: Vec<Inst>,
    ) -> JoinHandle<Result<(Vec<Fp>, Vec<Fp>), SomeError>> {
        thread::spawn(move || {
            let mut s = Machine {
                o_chans,
                i_chans,
                triple_chan,
                instructions,
                c_stack: vec![],
                s_stack: vec![],
                c_output: vec![],
                s_output: vec![],
            };
            s.listen()?;
            Ok((s.c_output, s.s_output))
        })
    }

    fn listen(&mut self) -> Result<(), SomeError> {
        // wait for start
        loop {
            let msg = self.i_chans.recv()?;
            if msg == SyncMsg::Start {
                println!("machine starting!");
                break;
            } else {
                println!("received {:?} while waiting to start", msg);
            }
        }

        // process instructions
        loop {
            select! {
                recv(self.triple_chan) -> _ => () /* TODO */,
                recv(self.i_chans) -> v => {
                    let msg: SyncMsg = v?;
                    match msg {
                        SyncMsg::Start => println!("ignoring start"),
                        SyncMsg::Next => {
                            match self.instructions.pop() {
                                None => {
                                    self.o_chans.send(SyncMsgReply::Done)?;
                                    break;
                                },
                                Some(inst) => {
                                    println!("processing {:?}", inst);
                                    eval(
                                        &inst,
                                        &mut self.c_stack,
                                        &mut self.s_stack,
                                        &mut self.c_output,
                                        &mut self.s_output,
                                    );
                                    self.o_chans.send(SyncMsgReply::Ok)?;
                                }
                            }
                        },
                        SyncMsg::Abort => panic!("abort"),
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ff::Field;

    #[test]
    fn test_eval_cadd() {
        let one = Fp::one();
        let two = one + one;
        let mut c_stack = vec![one, one];
        let mut s_stack = vec![];
        let mut c_output = vec![];
        let mut s_output = vec![];
        eval(
            &Inst::CAdd,
            &mut c_stack,
            &mut s_stack,
            &mut c_output,
            &mut s_output,
        );

        assert_eq!(c_stack.len(), 1);
        assert_eq!(c_stack[0], two);
    }

    // #[test]
    // fn test_abort() {
    //     let (from_sync, to_machine) = bounded(5);
    //     let (from_machine, to_sync) = bounded(5);
    //     let (_, triple_chan) = bounded(5);
    // }
}
