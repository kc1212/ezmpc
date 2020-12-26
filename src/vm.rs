use crate::algebra::Fp;
use crate::crypto::AuthShare;
use crate::error::{EvalError, SomeError};

use crossbeam_channel::{bounded, Receiver, Sender};
use num_traits::Zero;
use std::cmp::min;
use std::ops;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

type RegAddr = usize;
pub type PartyID = usize;

#[derive(Copy, Clone, Debug)]
pub struct Reg {
    clear: [Option<Fp>; REG_SIZE],
    secret: [Option<AuthShare>; REG_SIZE],
}

const REG_SIZE: usize = 128;

pub struct VM {
    alpha_share: Fp, // TODO could be a reference type
    reg: Reg,
    id: PartyID,
}

pub fn empty_reg() -> Reg {
    Reg {
        clear: [None; REG_SIZE],
        secret: [None; REG_SIZE],
    }
}

pub fn vec_to_reg(v: &Vec<Fp>) -> Reg {
    let mut clear = [None; REG_SIZE];
    let n = min(v.len(), REG_SIZE);
    for i in 0..n {
        clear[i] = Some(v[i]);
    }
    Reg {
        clear,
        secret: [None; REG_SIZE],
    }
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
    CAdd(RegAddr, RegAddr, RegAddr),            // clear add
    CSub(RegAddr, RegAddr, RegAddr),            // clear sub
    CAddTo(RegAddr, RegAddr, RegAddr, PartyID), // clear add to one party (TODO might not need this)
    CMul(RegAddr, RegAddr, RegAddr),            // clear mul
    SAdd(RegAddr, RegAddr, RegAddr),            // secret add
    SSub(RegAddr, RegAddr, RegAddr),            // secret sub
    MAdd(RegAddr, RegAddr, RegAddr, PartyID),   // mixed add: [a+b] <- a + [b]
    MMul(RegAddr, RegAddr, RegAddr),            // mixed mul: [a*b] <- a * [b]
    Triple(RegAddr, RegAddr, RegAddr),          // store triple
    Open(RegAddr, RegAddr),                     // open a shared/secret value
    COutput(RegAddr),                           // output a clear value
    SOutput(RegAddr),                           // output a secret value
    Stop,                                       // stop the VM
}

fn wrap_option<T>(v: Option<T>, err: EvalError) -> Result<T, SomeError> {
    match v {
        Some(x) => Ok(x),
        None => Err(err.into()),
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
        let alpha_share = Fp::zero(); // FIXME
        VM { alpha_share, reg, id }
    }

    // listen for incoming instructions, send some result back to sender
    fn listen(&mut self, i_chan: Receiver<Instruction>, o_chan: Sender<Action>) -> Result<Vec<Fp>, SomeError> {
        let mut output = Vec::new();

        loop {
            let inst = i_chan.recv_timeout(Duration::from_secs(1))?;
            match inst {
                Instruction::CAdd(r0, r1, r2) => o_chan.send(self.do_clear_op(r0, r1, r2, ops::Add::add)?)?,
                Instruction::CSub(r0, r1, r2) => o_chan.send(self.do_clear_op(r0, r1, r2, ops::Sub::sub)?)?,
                Instruction::CAddTo(r0, r1, r2, id) => o_chan.send(self.do_clear_op_for_party(r0, r1, r2, ops::Add::add, id)?)?,
                Instruction::CMul(r0, r1, r2) => o_chan.send(self.do_clear_op(r0, r1, r2, ops::Mul::mul)?)?,
                Instruction::SAdd(r0, r1, r2) => o_chan.send(self.do_secret_op(r0, r1, r2, ops::Add::add)?)?,
                Instruction::SSub(r0, r1, r2) => o_chan.send(self.do_secret_op(r0, r1, r2, ops::Sub::sub)?)?,
                Instruction::MAdd(r0, r1, r2, id) => o_chan.send(self.do_mixed_add(r0, r1, r2, id)?)?,
                Instruction::MMul(r0, r1, r2) => o_chan.send(self.do_mixed_mul(r0, r1, r2)?)?,
                Instruction::Triple(r0, r1, r2) => self.process_triple(r0, r1, r2, &o_chan)?,
                Instruction::Open(to, from) => self.process_open(to, from, &o_chan)?,
                Instruction::COutput(reg) => {
                    output.push(wrap_option(self.reg.clear[reg], EvalError::OutputEmptyReg)?);
                    o_chan.send(Action::None)?
                }
                Instruction::SOutput(reg) => {
                    output.push(wrap_option(self.reg.secret[reg], EvalError::OutputEmptyReg)?.share);
                    o_chan.send(Action::None)?
                }
                Instruction::Stop => return Ok(output),
            }
        }
    }

    fn do_clear_op<F>(&mut self, r0: RegAddr, r1: RegAddr, r2: RegAddr, op: F) -> Result<Action, SomeError>
    where
        F: Fn(Fp, Fp) -> Fp,
    {
        let c = self.reg.clear[r1].zip(self.reg.clear[r2]).map(|(a, b)| op(a, b));
        match c {
            None => Err(EvalError::OpEmptyReg.into()),
            Some(x) => {
                self.reg.clear[r0] = Some(x);
                Ok(Action::None)
            }
        }
    }

    fn do_clear_op_for_party<F>(&mut self, r0: RegAddr, r1: RegAddr, r2: RegAddr, op: F, id: PartyID) -> Result<Action, SomeError>
    where
        F: Fn(Fp, Fp) -> Fp,
    {
        if self.id == id {
            self.do_clear_op(r0, r1, r2, op)
        } else {
            // just copy the content from r1 to r0
            self.reg.clear[r0] = self.reg.clear[r1];
            Ok(Action::None)
        }
    }

    fn do_secret_op<F>(&mut self, r0: RegAddr, r1: RegAddr, r2: RegAddr, op: F) -> Result<Action, SomeError>
    where
        F: Fn(AuthShare, AuthShare) -> AuthShare,
    {
        let c = self.reg.secret[r1].zip(self.reg.secret[r2]).map(|(a, b)| op(a, b));
        match c {
            None => Err(EvalError::OpEmptyReg.into()),
            Some(x) => {
                self.reg.secret[r0] = Some(x);
                Ok(Action::None)
            }
        }
    }

    fn do_mixed_add(&mut self, s_r0: RegAddr, s_r1: RegAddr, c_r2: RegAddr, id: PartyID) -> Result<Action, SomeError> {
        let c = self.reg.secret[s_r1]
            .zip(self.reg.clear[c_r2])
            .map(|(a, b)| a.add_const(&b, &self.alpha_share, self.id == id));
        match c {
            None => Err(EvalError::OpEmptyReg.into()),
            Some(x) => {
                self.reg.secret[s_r0] = Some(x);
                Ok(Action::None)
            }
        }
    }

    fn do_mixed_mul(&mut self, s_r0: RegAddr, s_r1: RegAddr, c_r2: RegAddr) -> Result<Action, SomeError> {
        let c = self.reg.secret[s_r1].zip(self.reg.clear[c_r2]).map(|(a, b)| a.mul_const(&b));
        match c {
            None => Err(EvalError::OpEmptyReg.into()),
            Some(x) => {
                self.reg.secret[s_r0] = Some(x);
                Ok(Action::None)
            }
        }
    }

    fn process_triple(&mut self, r0: RegAddr, r1: RegAddr, r2: RegAddr, o_chan: &Sender<Action>) -> Result<(), SomeError> {
        let (s, r) = bounded(1);
        o_chan.send(Action::Triple(s))?;

        // wait for the triple
        let triple: (Fp, Fp, Fp) = r.recv_timeout(Duration::from_secs(1))?;
        self.reg.clear[r0] = Some(triple.0);
        self.reg.clear[r1] = Some(triple.1);
        self.reg.clear[r2] = Some(triple.2);
        Ok(())
    }

    fn process_open(&mut self, to: RegAddr, from: RegAddr, o_chan: &Sender<Action>) -> Result<(), SomeError> {
        match self.reg.clear[from] {
            None => Err(EvalError::OpenEmptyReg.into()),
            Some(for_opening) => {
                let (s, r) = bounded(1);
                o_chan.send(Action::Open(for_opening, s))?;

                // wait for the response
                // TODO parameterize these timeouts
                let opened: Fp = r.recv_timeout(Duration::from_secs(1))?;
                self.reg.clear[to] = Some(opened);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::{One, Zero};
    use rand::{Rng, SeedableRng, XorShiftRng};

    const SEED: [u32; 4] = [0x5dbe6259, 0x8d313d76, 0x3237db17, 0xe5bc0654];

    fn simple_vm_runner(prog: Vec<Instruction>, reg: Reg) -> Result<Vec<Fp>, SomeError> {
        let (_, dummy_open_chan) = bounded(5);
        let (_, dummy_triple_chan) = bounded(5);
        vm_runner(prog, reg, dummy_open_chan, dummy_triple_chan)
    }

    fn vm_runner(prog: Vec<Instruction>, reg: Reg, open_chan: Receiver<Fp>, triple_chan: Receiver<(Fp, Fp, Fp)>) -> Result<Vec<Fp>, SomeError> {
        let (s_instruction_chan, r_instruction_chan) = bounded(5);
        let (s_action_chan, r_action_chan) = bounded(5);

        let handle = VM::spawn(0, reg, r_instruction_chan, s_action_chan);
        for instruction in prog {
            s_instruction_chan.send(instruction)?;
            if instruction == Instruction::Stop {
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

    /// this function should be used only in testing, it doesn't care about the MACs
    fn vec_to_secret_reg(v: &Vec<Fp>) -> Reg {
        let mut secret = [None; REG_SIZE];
        let n = min(v.len(), REG_SIZE);
        for i in 0..n {
            let share = AuthShare {
                share: v[i],
                mac: Fp::zero(),
            };
            secret[i] = Some(share);
        }
        Reg {
            clear: [None; REG_SIZE],
            secret,
        }
    }

    fn compute_secret_op<F>(a: Fp, b: Fp, op: F) -> Fp
    where
        F: Fn(RegAddr, RegAddr, RegAddr) -> Instruction,
    {
        let prog = vec![op(2, 1, 0), Instruction::SOutput(2), Instruction::Stop];
        let reg = vec_to_secret_reg(&vec![a, b]);
        let result = simple_vm_runner(prog, reg).unwrap();
        assert_eq!(result.len(), 1);
        result[0]
    }

    fn compute_clear_op<F>(a: Fp, b: Fp, op: F) -> Fp
    where
        F: Fn(RegAddr, RegAddr, RegAddr) -> Instruction,
    {
        let prog = vec![op(2, 1, 0), Instruction::COutput(2), Instruction::Stop];
        let reg = vec_to_reg(&vec![a, b]);
        let result = simple_vm_runner(prog, reg).unwrap();
        assert_eq!(result.len(), 1);
        result[0]
    }

    fn compute_add_to_party(a: Fp, b: Fp, id: PartyID) -> Fp {
        let prog = vec![Instruction::CAddTo(2, 1, 0, id), Instruction::COutput(2), Instruction::Stop];

        let reg = vec_to_reg(&vec![a, b]);
        let result = simple_vm_runner(prog, reg).unwrap();
        assert_eq!(result.len(), 1);
        result[0]
    }

    #[test]
    fn test_c_add() {
        let op = |x, y, z| Instruction::CAdd(x, y, z);
        let one = Fp::one();
        assert_eq!(compute_clear_op(one, one, op), one + one);

        let rng = &mut XorShiftRng::from_seed(SEED);
        let a = rng.gen();
        let b = rng.gen();
        assert_eq!(compute_clear_op(a, b, op), a + b);
    }

    #[test]
    fn test_c_mul() {
        let op = |x, y, z| Instruction::CMul(x, y, z);
        assert_eq!(compute_clear_op(Fp::one(), Fp::zero(), op), Fp::zero());

        let rng = &mut XorShiftRng::from_seed(SEED);
        let a = rng.gen();
        let b = rng.gen();
        assert_eq!(compute_clear_op(a, b, op), a * b);
    }

    #[test]
    fn test_c_sub() {
        let op = |x, y, z| Instruction::CSub(x, y, z);
        assert_eq!(compute_clear_op(Fp::one(), Fp::one(), op), Fp::zero());

        let rng = &mut XorShiftRng::from_seed(SEED);
        let a = rng.gen();
        let b = rng.gen();
        assert_eq!(compute_clear_op(a, b, op), b - a);
    }

    #[test]
    fn test_c_add_to_party() {
        let one = Fp::one();
        assert_eq!(compute_add_to_party(one, one, 0), one + one);
        assert_eq!(compute_add_to_party(one, one, 1), one);
    }

    #[test]
    fn test_s_add() {
        let op = |x, y, z| Instruction::SAdd(x, y, z);
        let one = Fp::one();
        assert_eq!(compute_secret_op(one, one, op), one + one);

        let rng = &mut XorShiftRng::from_seed(SEED);
        let a = rng.gen();
        let b = rng.gen();
        assert_eq!(compute_secret_op(a, b, op), a + b);
    }

    #[test]
    fn test_s_sub() {
        let op = |x, y, z| Instruction::SSub(x, y, z);
        let one = Fp::one();
        assert_eq!(compute_secret_op(one, one, op), one - one);

        let rng = &mut XorShiftRng::from_seed(SEED);
        let a = rng.gen();
        let b = rng.gen();
        assert_eq!(compute_secret_op(a, b, op), b - a);
    }

    #[test]
    fn test_m_add() {
        let s1 = Fp::one();
        let c2 = Fp::one();
        let mut reg = vec_to_secret_reg(&vec![s1]);
        reg.clear[0] = Some(c2);
        {
            // use id = 0
            let prog = vec![Instruction::MAdd(1, 0, 0, 0), Instruction::SOutput(1), Instruction::Stop];

            let result = simple_vm_runner(prog, reg).unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0], s1 + c2);
        }
        {
            // use id = 1
            let prog = vec![Instruction::MAdd(1, 0, 0, 1), Instruction::SOutput(1), Instruction::Stop];

            let result = simple_vm_runner(prog, reg).unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0], s1); // shouldn't be added
        }
    }

    #[test]
    fn test_m_mul() {
        let s1 = Fp::one();
        let c2 = Fp::one() + Fp::one();
        let mut reg = vec_to_secret_reg(&vec![s1]);
        reg.clear[0] = Some(c2);

        let prog = vec![Instruction::MMul(1, 0, 0), Instruction::SOutput(1), Instruction::Stop];

        let result = simple_vm_runner(prog, reg).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], s1 * c2);
    }

    #[test]
    fn test_open() {
        let prog = vec![Instruction::Open(0, 0), Instruction::COutput(0), Instruction::Stop];
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
            Instruction::Triple(0, 1, 2),
            Instruction::COutput(0),
            Instruction::COutput(1),
            Instruction::COutput(2),
            Instruction::Stop,
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
