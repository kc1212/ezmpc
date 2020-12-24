use crate::crypto::Fp;
use crate::error::SomeError;
use crossbeam_channel::{Receiver, Sender, bounded};
use std::time::Duration;
use std::thread;
use std::thread::JoinHandle;
use std::cmp::min;

type RegAddr= usize;
pub type PartyID = usize;

pub type Reg = [Option<Fp>; REG_SIZE];

pub fn empty_reg() -> Reg {
    [None; REG_SIZE]
}

pub fn vec_to_reg(v: &Vec<Fp>) -> Reg {
    let mut reg = [None; REG_SIZE];
    let n = min(v.len(), REG_SIZE);
    for i in 0..n {
        reg[i] = Some(v[i]);
    }
    reg
}

const REG_SIZE: usize = 128;

pub struct VM {
    register: Reg,
    id: PartyID,
}

// TODO might be a problem for error handling if we cannot derive Eq/PartialEq
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
    pub fn spawn(id: PartyID, reg: Reg, i_chan: Receiver<Instruction>, o_chan: Sender<Action>) -> JoinHandle<Result<Vec<Fp>, SomeError>> {
        thread::spawn(move || {
            let mut vm = VM::new(id, reg);
            vm.listen(i_chan, o_chan)
        })
    }

    fn new(id: PartyID, reg: Reg) -> VM {
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
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng, XorShiftRng};
    const SEED: [u32; 4] = [0x5dbe6259, 0x8d313d76, 0x3237db17, 0xe5bc0654];
    use ff::Field;

    fn simple_vm_runner(instructions: Vec<Instruction>, reg: Reg) -> Result<Vec<Fp>, SomeError> {
        let (_, dummy_open_chan) = bounded(5);
        let (_, dummy_triple_chan) = bounded(5);
        vm_runner(instructions, reg, dummy_open_chan, dummy_triple_chan)
    }

    fn vm_runner(instructions: Vec<Instruction>, reg: Reg, open_chan: Receiver<Fp>, triple_chan: Receiver<(Fp, Fp, Fp)>) -> Result<Vec<Fp>, SomeError> {
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
                Action::Open(_, sender) => {
                    let x = open_chan.recv_timeout(Duration::from_secs(1))?;
                    sender.send(x)?
                }
                Action::Triple(sender) => {
                    let triple = triple_chan.recv_timeout(Duration::from_secs(1))?;
                    sender.send(triple)?
                }
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
        let reg = vec_to_reg(&vec![a, b]);
        let result = simple_vm_runner(prog, reg).unwrap();
        assert_eq!(result.len(), 1);
        result[0]
    }

    fn compute_add_to_party(a: Fp, b: Fp, id: PartyID) -> Fp {
        let prog = vec![
            Instruction::ADDP(2, 1, 0, id),
            Instruction::OUTPUT(2),
            Instruction::STOP,
        ];

        let reg = vec_to_reg(&vec![a, b]);
        let result = simple_vm_runner(prog, reg).unwrap();
        assert_eq!(result.len(), 1);
        result[0]
    }

    #[test]
    fn test_add() {
        let one = Fp::one();
        assert_eq!(compute_op(one, one, true), one + one);

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

    #[test]
    fn test_add_to_party() {
        let one = Fp::one();
        assert_eq!(compute_add_to_party(one, one, 0), one + one);
        assert_eq!(compute_add_to_party(one, one, 1), one);
    }

    #[test]
    fn test_open() {
        let prog = vec![
            Instruction::OPEN(0, 0),
            Instruction::OUTPUT(0),
            Instruction::STOP,
        ];
        let reg = vec_to_reg(&vec![Fp::one()]);

        let (s, r) = bounded(5);
        let (_, dummy_triple_chan) = bounded(5);
        s.send(Fp::zero()).unwrap();
        let result = vm_runner(prog, reg, r, dummy_triple_chan).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], Fp::zero());
    }

    #[test]
    fn test_triple() {
        let prog = vec![
            Instruction::TRIPLE(0, 1, 2),
            Instruction::OUTPUT(0),
            Instruction::OUTPUT(1),
            Instruction::OUTPUT(2),
            Instruction::STOP,
        ];

        let (s, r) = bounded(5);
        let (_, dummy_open_chan) = bounded(5);
        let zero = Fp::zero();
        let one = Fp::one();
        let two = one + one;
        s.send((zero, one, two)).unwrap();
        let result = vm_runner(prog, empty_reg(), dummy_open_chan, r).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], zero);
        assert_eq!(result[1], one);
        assert_eq!(result[2], two);
    }

    // TODO test for failures
}