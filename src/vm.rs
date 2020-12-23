use crate::crypto::Fp;
use crate::error::SomeError;
use crossbeam_channel::{Receiver, Sender, bounded};
use std::time::Duration;
use std::thread;
use std::thread::JoinHandle;

type RegAddr= usize;
type PartyID = usize;

const REG_SIZE: usize = 128;

struct VM {
    register: [Option<Fp>; REG_SIZE],
    id: PartyID,
}

#[derive(Debug)]
pub enum Action {
    None,
    Open(Fp, Sender<Fp>),
    Triple(Sender<(Fp, Fp, Fp)>),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Instruction {
    ADD(RegAddr, RegAddr, RegAddr),
    ADDP(RegAddr, RegAddr, RegAddr, PartyID),
    MUL(RegAddr, RegAddr, RegAddr),
    TRIPLE(RegAddr, RegAddr, RegAddr),
    OPEN(RegAddr, RegAddr),
    OUTPUT(RegAddr),
    STOP,
}

fn wrap_option<T>(v: Option<T>) -> Result<T, SomeError> {
    match v {
        Some(x) => Ok(x),
        None => Err(SomeError::NoneError),
    }
}

impl VM {
    pub fn spawn(id: PartyID, reg: [Option<Fp>; REG_SIZE], i_chan: Receiver<Instruction>, o_chan: Sender<Action>) -> JoinHandle<Result<Vec<Fp>, SomeError>> {
        thread::spawn(move || {
            let mut vm = VM::new(id, reg);
            vm.listen(i_chan, o_chan)
        })
    }

    fn new(id: PartyID, reg: [Option<Fp>; REG_SIZE]) -> VM {
        VM {
            register: reg,
            id,
        }
    }

    // listen for incoming instructions, send some result back to sender
    fn listen(&mut self, i_chan: Receiver<Instruction>, o_chan: Sender<Action>) -> Result<Vec<Fp>, SomeError> {
        let addop = |x: &Fp, y: &Fp| x + y;
        let mulop = |x: &Fp, y: &Fp| x * y;
        let mut output = Vec::new();

        loop {
            let inst = i_chan.recv_timeout(Duration::from_secs(1))?;
            match inst {
                Instruction::ADD(r0, r1, r2) =>
                    o_chan.send(self.do_op(r0, r1, r2, addop)?)?,
                Instruction::ADDP(r0, r1, r2, id) =>
                    o_chan.send(self.do_op_for_party(r0, r1, r2, addop, id)?)?,
                Instruction::MUL(r0, r1, r2) =>
                    o_chan.send(self.do_op(r0, r1, r2, mulop)?)?,
                Instruction::TRIPLE(r0, r1, r2) =>
                    self.process_triple(r0, r1, r2, &o_chan)?,
                Instruction::OPEN(to, from) =>
                    self.process_open(to, from, &o_chan)?,
                Instruction::OUTPUT(reg) => {
                    output.push(wrap_option(self.register[reg])?);
                    o_chan.send(Action::None)?
                }
                Instruction::STOP =>
                    return Ok(output),
            }
        }
    }

    fn do_op<F>(&mut self, r0: RegAddr, r1: RegAddr, r2: RegAddr, op: F) -> Result<Action, SomeError>
        where F: Fn(&Fp, &Fp) -> Fp
    {
        let c = self.register[r1]
            .zip(self.register[r2]).map(|(a, b)| op(&a, &b));
        match c {
            None => {
                Err(SomeError::EvalError)
            }
            Some(x) => {
                self.register[r0] = Some(x);
                Ok(Action::None)
            }
        }
    }

    fn do_op_for_party<F>(&mut self, r0: RegAddr, r1: RegAddr, r2: RegAddr, op: F, id: PartyID) -> Result<Action, SomeError>
        where F: Fn(&Fp, &Fp) -> Fp
    {
        if self.id == id  {
            self.do_op(r0, r1, r2, op)
        } else {
            // just copy the content from r1 to r0
            self.register[r0] = self.register[r1];
            Ok(Action::None)
        }
    }

    fn process_triple(&mut self, r0: RegAddr, r1: RegAddr, r2: RegAddr, o_chan: &Sender<Action>) -> Result<(), SomeError> {
        let (s, r) = bounded(1);
        o_chan.send(Action::Triple(s))?;

        // wait for the triple
        let triple: (Fp, Fp, Fp) = r.recv_timeout(Duration::from_secs(1))?;
        self.register[r0] = Some(triple.0);
        self.register[r1] = Some(triple.1);
        self.register[r2] = Some(triple.2);
        Ok(())
    }

    fn process_open(&mut self, to: RegAddr, from: RegAddr, o_chan: &Sender<Action>) -> Result<(), SomeError> {
        match self.register[from] {
            None => {
                Err(SomeError::EvalError)
            }
            Some(for_opening) => {
                let (s, r) = bounded(1);
                o_chan.send(Action::Open(for_opening, s))?;

                // wait for the response
                // TODO parameterize these timeouts
                let opened: Fp = r.recv_timeout(Duration::from_secs(1))?;
                self.register[to] = Some(opened);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::{Rng, SeedableRng, XorShiftRng};
    const SEED: [u32; 4] = [0x5dbe6259, 0x8d313d76, 0x3237db17, 0xe5bc0654];
    use ff::Field;

    fn single_vm_runner(instructions: Vec<Instruction>, reg: [Option<Fp>; REG_SIZE]) -> Result<Vec<Fp>, SomeError> {
        let (s_instruction_chan, r_instruction_chan) = bounded(5);
        let (s_action_chan, r_action_chan) = bounded(5);

        let handle = VM::spawn(0, reg, r_instruction_chan, s_action_chan);
        for instruction in instructions {
            s_instruction_chan.send(instruction)?;
            if instruction == Instruction::STOP {
                break;
            }
            let reply = r_action_chan.recv_timeout(Duration::from_secs(1))?;
            match reply {
                Action::None => (),
                _ => panic!("unexpected action"),
            }
        }

        handle.join().unwrap()
    }

    fn compute_op(a: Fp, b: Fp, is_add: bool) -> Fp {
        let prog = vec![
            if is_add {Instruction::ADD(2, 1, 0)} else {Instruction::MUL(2, 1, 0)},
            Instruction::OUTPUT(2),
            Instruction::STOP,
        ];
        let mut reg= [None; REG_SIZE];
        reg[0] = Some(a);
        reg[1] = Some(b);
        let result = single_vm_runner(prog, reg).unwrap();
        assert_eq!(result.len(), 1);
        result[0]
    }

    #[test]
    fn test_add() {
        assert_eq!(compute_op(Fp::one(), Fp::one(), true), Fp::one() + Fp::one());

        let rng = &mut XorShiftRng::from_seed(SEED);
        let r0 = rng.gen();
        let r1 = rng.gen();
        assert_eq!(compute_op(r0, r1, true), r0 + r1);
    }

    #[test]
    fn test_mul() {
        assert_eq!(compute_op(Fp::one(), Fp::zero(), false), Fp::zero());

        let rng = &mut XorShiftRng::from_seed(SEED);
        let r0 = rng.gen();
        let r1 = rng.gen();
        assert_eq!(compute_op(r0, r1, false), r0 * r1);
    }
}