use crate::algebra::Fp;
use crate::crypto::AuthShare;
use crate::error::{OutputError, SomeError, TIMEOUT};
use crate::message::{InputRandMsg, PartyID};

use crossbeam_channel::{bounded, select, Receiver, Sender};
use num_traits::Zero;
use std::cmp::min;
use std::collections::HashMap;
use std::ops;
use std::thread;
use std::thread::JoinHandle;

type RegAddr = usize;

#[derive(Copy, Clone, Debug)]
pub struct Reg {
    clear: [Option<Fp>; REG_SIZE],
    secret: [Option<AuthShare>; REG_SIZE],
}

const REG_SIZE: usize = 128;

pub struct VM {
    id: PartyID,
    alpha_share: Fp, // could be a reference type
    reg: Reg,
    triple_chan: Receiver<(AuthShare, AuthShare, AuthShare)>,
    rand_chan: Receiver<InputRandMsg>,
    rand_msgs: HashMap<PartyID, Vec<InputRandMsg>>,
    partial_openings: Vec<AuthShare>,
}

pub fn empty_reg() -> Reg {
    Reg {
        clear: [None; REG_SIZE],
        secret: [None; REG_SIZE],
    }
}

pub fn vec_to_reg(vclear: &Vec<Fp>, vsecret: &Vec<AuthShare>) -> Reg {
    let mut clear = [None; REG_SIZE];
    let mut secret = [None; REG_SIZE];
    let cn = min(vclear.len(), REG_SIZE);
    for i in 0..cn {
        clear[i] = Some(vclear[i]);
    }
    let sn = min(vsecret.len(), REG_SIZE);
    for i in 0..sn {
        secret[i] = Some(vsecret[i]);
    }
    Reg { clear, secret }
}

pub fn unauth_vec_to_reg(vclear: &Vec<Fp>, vsecret: &Vec<Fp>) -> Reg {
    let vv: Vec<_> = vsecret
        .iter()
        .map(|x| AuthShare {
            share: *x,
            mac: Zero::zero(),
        })
        .collect();
    vec_to_reg(vclear, &vv)
}

// might be a problem for error handling if we cannot derive Eq/PartialEq
#[derive(Clone, Debug)]
pub enum Action {
    None,
    Open(Fp, Sender<Fp>),
    Input(PartyID, Option<Fp>, Sender<Fp>),
    SOutput(AuthShare, Sender<Result<(), OutputError>>),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Instruction {
    CAdd(RegAddr, RegAddr, RegAddr),          // clear add
    CSub(RegAddr, RegAddr, RegAddr),          // clear sub
    CMul(RegAddr, RegAddr, RegAddr),          // clear mul
    SAdd(RegAddr, RegAddr, RegAddr),          // secret add
    SSub(RegAddr, RegAddr, RegAddr),          // secret sub
    MAdd(RegAddr, RegAddr, RegAddr, PartyID), // mixed add: [a+b] <- a + [b]
    MMul(RegAddr, RegAddr, RegAddr),          // mixed mul: [a*b] <- a * [b]
    Input(RegAddr, RegAddr, PartyID),         // input value
    Triple(RegAddr, RegAddr, RegAddr),        // store triple
    Open(RegAddr, RegAddr),                   // open a shared/secret value
    COutput(RegAddr),                         // output a clear value
    SOutput(RegAddr),                         // output a secret value
    Stop,                                     // stop the VM
}

fn opt_to_res<T>(v: Option<T>) -> Result<T, SomeError> {
    match v {
        Some(x) => Ok(x),
        None => Err(SomeError::EmptyError),
    }
}

impl VM {
    pub fn spawn(
        id: PartyID,
        alpha_share: Fp,
        reg: Reg,
        triple_chan: Receiver<(AuthShare, AuthShare, AuthShare)>,
        rand_chan: Receiver<InputRandMsg>,
        r_chan: Receiver<Instruction>,
        s_chan: Sender<Action>,
    ) -> JoinHandle<Result<Vec<Fp>, SomeError>> {
        thread::spawn(move || {
            let mut vm = VM::new(id, alpha_share, reg, triple_chan, rand_chan);
            vm.listen(r_chan, s_chan)
        })
    }

    fn new(
        id: PartyID,
        alpha_share: Fp,
        reg: Reg,
        triple_chan: Receiver<(AuthShare, AuthShare, AuthShare)>,
        rand_chan: Receiver<InputRandMsg>,
    ) -> VM {
        VM {
            id,
            alpha_share,
            reg,
            triple_chan,
            rand_chan,
            rand_msgs: HashMap::new(),
            partial_openings: Vec::new(),
        }
    }

    // listen for incoming instructions, send some result back to sender
    fn listen(&mut self, r_chan: Receiver<Instruction>, s_chan: Sender<Action>) -> Result<Vec<Fp>, SomeError> {
        let mut output = Vec::new();

        loop {
            let inst = r_chan.recv_timeout(TIMEOUT)?;
            match inst {
                Instruction::CAdd(r0, r1, r2) => s_chan.send(self.do_clear_op(r0, r1, r2, ops::Add::add)?)?,
                Instruction::CSub(r0, r1, r2) => s_chan.send(self.do_clear_op(r0, r1, r2, ops::Sub::sub)?)?,
                Instruction::CMul(r0, r1, r2) => s_chan.send(self.do_clear_op(r0, r1, r2, ops::Mul::mul)?)?,
                Instruction::SAdd(r0, r1, r2) => s_chan.send(self.do_secret_op(r0, r1, r2, ops::Add::add)?)?,
                Instruction::SSub(r0, r1, r2) => s_chan.send(self.do_secret_op(r0, r1, r2, ops::Sub::sub)?)?,
                Instruction::MAdd(r0, r1, r2, id) => s_chan.send(self.do_mixed_add(r0, r1, r2, id)?)?,
                Instruction::MMul(r0, r1, r2) => s_chan.send(self.do_mixed_mul(r0, r1, r2)?)?,
                Instruction::Input(r0, r1, id) => self.process_input(r0, r1, id, &s_chan)?,
                Instruction::Triple(r0, r1, r2) => self.process_triple(r0, r1, r2, &s_chan)?,
                Instruction::Open(to, from) => self.process_open(to, from, &s_chan)?,
                Instruction::COutput(reg) => {
                    output.push(opt_to_res(self.reg.clear[reg])?);
                    s_chan.send(Action::None)?
                }
                Instruction::SOutput(reg) => {
                    let result = self.process_secret_output(reg, &s_chan)?;
                    output.push(result);
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
        self.reg.clear[r0] = Some(opt_to_res(c)?);
        Ok(Action::None)
    }

    fn do_secret_op<F>(&mut self, r0: RegAddr, r1: RegAddr, r2: RegAddr, op: F) -> Result<Action, SomeError>
    where
        F: Fn(AuthShare, AuthShare) -> AuthShare,
    {
        let c = self.reg.secret[r1].zip(self.reg.secret[r2]).map(|(a, b)| op(a, b));
        self.reg.secret[r0] = Some(opt_to_res(c)?);
        Ok(Action::None)
    }

    fn do_mixed_add(&mut self, s_r0: RegAddr, s_r1: RegAddr, c_r2: RegAddr, id: PartyID) -> Result<Action, SomeError> {
        let c = self.reg.secret[s_r1]
            .zip(self.reg.clear[c_r2])
            .map(|(a, b)| a.add_const(&b, &self.alpha_share, self.id == id));
        self.reg.secret[s_r0] = Some(opt_to_res(c)?);
        Ok(Action::None)
    }

    fn do_mixed_mul(&mut self, s_r0: RegAddr, s_r1: RegAddr, c_r2: RegAddr) -> Result<Action, SomeError> {
        let c = self.reg.secret[s_r1].zip(self.reg.clear[c_r2]).map(|(a, b)| a.mul_const(&b));
        self.reg.secret[s_r0] = Some(opt_to_res(c)?);
        Ok(Action::None)
    }

    fn get_rand_share_for_id(&mut self, id: PartyID) -> Result<InputRandMsg, SomeError> {
        loop {
            select! {
                recv(self.rand_chan) -> r_res => {
                    // create empty entry if id does not exist
                    let r = r_res?;
                    if !self.rand_msgs.contains_key(&r.party_id) {
                        self.rand_msgs.insert(r.party_id, vec![]);
                    }
                    // write the msg
                    match self.rand_msgs.get_mut(&r.party_id) {
                        Some(v) => v.push(r),
                        None => panic!("rand share for id {} should exist", r.party_id),
                    }
                }
                default => {
                    break;
                }
            }
        }
        // not that we're reading the rand msgs in a LIFO order
        let opt_out = opt_to_res(self.rand_msgs.get_mut(&id))?.pop();
        opt_to_res(opt_out)
    }

    fn process_input(&mut self, r0: RegAddr, r1: RegAddr, id: PartyID, s_chan: &Sender<Action>) -> Result<(), SomeError> {
        let rand_share = self.get_rand_share_for_id(id)?;

        let (s, r) = bounded(1);
        if self.id == id {
            let x = opt_to_res(self.reg.clear[r1])?;
            let e = x - opt_to_res(rand_share.clear)?;
            s_chan.send(Action::Input(id, Some(e), s))?;
        } else {
            s_chan.send(Action::Input(id, None, s))?;
        }

        let e = r.recv_timeout(TIMEOUT)?;
        let input_share = rand_share.share.add_const(&e, &self.alpha_share, self.id == id);
        self.reg.secret[r0] = Some(input_share);
        Ok(())
    }

    fn process_triple(&mut self, r0: RegAddr, r1: RegAddr, r2: RegAddr, s_chan: &Sender<Action>) -> Result<(), SomeError> {
        let triple = self.triple_chan.recv_timeout(TIMEOUT)?;
        self.reg.secret[r0] = Some(triple.0);
        self.reg.secret[r1] = Some(triple.1);
        self.reg.secret[r2] = Some(triple.2);
        s_chan.send(Action::None)?;
        Ok(())
    }

    fn process_open(&mut self, to: RegAddr, from: RegAddr, s_chan: &Sender<Action>) -> Result<(), SomeError> {
        match self.reg.secret[from] {
            None => Err(SomeError::EmptyError),
            Some(for_opening) => {
                let (s, r) = bounded(1);
                s_chan.send(Action::Open(for_opening.share, s))?;

                // wait for the response
                let opened: Fp = r.recv_timeout(TIMEOUT)?;
                self.reg.clear[to] = Some(opened);

                // store the opened value for maccheck later
                self.partial_openings.push(for_opening);
                Ok(())
            }
        }
    }

    fn process_secret_output(&mut self, reg: RegAddr, s_chan: &Sender<Action>) -> Result<Fp, SomeError> {
        let share = match self.reg.secret[reg] {
            None => Err(OutputError::RegisterEmpty),
            Some(x) => Ok(x),
        }?;
        let (s, r) = bounded(5);
        s_chan.send(Action::SOutput(share, s))?;

        // wait for response
        r.recv_timeout(TIMEOUT)??;
        Ok(share.share)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::Zero;

    fn simple_vm_runner(prog: Vec<Instruction>, reg: Reg) -> Result<Vec<Fp>, SomeError> {
        let (_, dummy_triple_chan) = bounded(5);
        let (_, dummy_open_chan) = bounded(5);
        let (_, dummy_rand_chan) = bounded(5);
        vm_runner(prog, reg, dummy_triple_chan, dummy_open_chan, dummy_rand_chan)
    }

    fn vm_runner(
        prog: Vec<Instruction>,
        reg: Reg,
        triple_chan: Receiver<(AuthShare, AuthShare, AuthShare)>,
        rand_chan: Receiver<InputRandMsg>,
        open_chan: Receiver<Fp>,
    ) -> Result<Vec<Fp>, SomeError> {
        let (s_instruction_chan, r_instruction_chan) = bounded(5);
        let (s_action_chan, r_action_chan) = bounded(5);

        let fake_alpha_share = Fp::zero();
        let handle = VM::spawn(0, fake_alpha_share, reg, triple_chan, rand_chan, r_instruction_chan, s_action_chan);
        for instruction in prog {
            s_instruction_chan.send(instruction)?;
            if instruction == Instruction::Stop {
                break;
            }

            // these replies are obviously not the correct implementation, they're only here for testing
            // the actual implementation is in node.rs
            let reply = r_action_chan.recv_timeout(TIMEOUT)?;
            match reply {
                Action::None => (),
                Action::Open(_, sender) => {
                    let x = open_chan.recv_timeout(TIMEOUT)?;
                    sender.send(x)?
                }
                Action::Input(_, e_option, sender) => match e_option {
                    Some(e) => sender.send(e)?,
                    None => sender.send(Fp::zero())?,
                },
                Action::SOutput(_, sender) => sender.send(Ok(()))?,
            }
        }

        handle.join().unwrap()
    }

    fn compute_secret_op<F>(a: Fp, b: Fp, op: F) -> Fp
    where
        F: Fn(RegAddr, RegAddr, RegAddr) -> Instruction,
    {
        let prog = vec![op(2, 1, 0), Instruction::SOutput(2), Instruction::Stop];
        let reg = unauth_vec_to_reg(&vec![], &vec![a, b]);
        let result = simple_vm_runner(prog, reg).unwrap();
        assert_eq!(result.len(), 1);
        result[0]
    }

    fn compute_clear_op<F>(a: Fp, b: Fp, op: F) -> Fp
    where
        F: Fn(RegAddr, RegAddr, RegAddr) -> Instruction,
    {
        let prog = vec![op(2, 1, 0), Instruction::COutput(2), Instruction::Stop];
        let reg = vec_to_reg(&vec![a, b], &vec![]);
        let result = simple_vm_runner(prog, reg).unwrap();
        assert_eq!(result.len(), 1);
        result[0]
    }

    #[quickcheck]
    fn prop_clear_add(x: Fp, y: Fp) -> bool {
        let op = |x, y, z| Instruction::CAdd(x, y, z);
        compute_clear_op(x, y, op) == x + y
    }

    #[quickcheck]
    fn prop_clear_mul(x: Fp, y: Fp) -> bool {
        let op = |x, y, z| Instruction::CMul(x, y, z);
        compute_clear_op(x, y, op) == x * y
    }

    #[quickcheck]
    fn prop_clear_sub(x: Fp, y: Fp) -> bool {
        let op = |x, y, z| Instruction::CSub(x, y, z);
        compute_clear_op(x, y, op) == y - x
    }

    #[quickcheck]
    fn prop_secret_add(x: Fp, y: Fp) -> bool {
        let op = |x, y, z| Instruction::SAdd(x, y, z);
        compute_secret_op(x, y, op) == x + y
    }

    #[quickcheck]
    fn prop_secret_sub(x: Fp, y: Fp) -> bool {
        let op = |x, y, z| Instruction::SSub(x, y, z);
        compute_secret_op(x, y, op) == y - x
    }

    #[quickcheck]
    fn prop_mixed_add(s1: Fp, c2: Fp, id: PartyID) -> bool {
        let reg = unauth_vec_to_reg(&vec![c2], &vec![s1]);

        // use id = 0
        let prog = vec![Instruction::MAdd(1, 0, 0, id), Instruction::SOutput(1), Instruction::Stop];
        let result = simple_vm_runner(prog, reg).unwrap();
        assert_eq!(result.len(), 1);
        if id == 0 {
            result[0] == s1 + c2
        } else {
            result[0] == s1
        }
    }

    #[quickcheck]
    fn prop_mixed_mul(s1: Fp, c2: Fp) -> bool {
        let reg = unauth_vec_to_reg(&vec![c2], &vec![s1]);

        let prog = vec![Instruction::MMul(1, 0, 0), Instruction::SOutput(1), Instruction::Stop];

        let result = simple_vm_runner(prog, reg).unwrap();
        assert_eq!(result.len(), 1);
        result[0] == s1 * c2
    }

    #[quickcheck]
    fn prop_open(s: Fp, v: Fp) -> bool {
        let prog = vec![Instruction::Open(0, 0), Instruction::COutput(0), Instruction::Stop];
        let reg = unauth_vec_to_reg(&vec![], &vec![s]);

        let (_, dummy_triple_chan) = bounded(5);
        let (_, dummy_rand_chan) = bounded(5);
        let (s_open_chan, r_open_chan) = bounded(5);

        s_open_chan.send(v).unwrap();
        let result = vm_runner(prog, reg, dummy_triple_chan, dummy_rand_chan, r_open_chan).unwrap();

        // the result should be whatever we send in the s_open_chan,
        // regardless of what the initial secret value is
        result.len() == 1 && result[0] == v
    }

    #[quickcheck]
    fn prop_triple(a: Fp, b: Fp, c: Fp) -> bool {
        let prog = vec![
            Instruction::Triple(0, 1, 2),
            Instruction::SOutput(0),
            Instruction::SOutput(1),
            Instruction::SOutput(2),
            Instruction::Stop,
        ];

        let (s_triple_chan, r_triple_chan) = bounded(5);
        let (_, dummy_rand_chan) = bounded(5);
        let (_, dummy_open_chan) = bounded(5);

        let a_share = AuthShare { share: a, mac: Fp::zero() };
        let b_share = AuthShare { share: b, mac: Fp::zero() };
        let c_share = AuthShare { share: c, mac: Fp::zero() };
        s_triple_chan.send((a_share, b_share, c_share)).unwrap();
        let result = vm_runner(prog, empty_reg(), r_triple_chan, dummy_rand_chan, dummy_open_chan).unwrap();
        result.len() == 3 && result[0] == a_share.share && result[1] == b_share.share && result[2] == c_share.share
    }

    #[quickcheck]
    fn prop_input(r: Fp, r_share: Fp, x: Fp) -> bool {
        let prog = vec![Instruction::Input(0, 0, 0), Instruction::SOutput(0), Instruction::Stop];

        let (_, dummy_triple_chan) = bounded(5);
        let (s_rand_chan, r_rand_chan) = bounded(5);
        let (_, dummy_open_chan) = bounded(5);

        let rand_msg = InputRandMsg {
            share: AuthShare {
                share: r_share,
                mac: Fp::zero(),
            },
            clear: Some(r),
            party_id: 0,
        };
        s_rand_chan.send(rand_msg).unwrap();
        let result = vm_runner(
            prog,
            unauth_vec_to_reg(&vec![x], &vec![]),
            dummy_triple_chan,
            r_rand_chan,
            dummy_open_chan,
        )
        .unwrap();

        // for rand_msg, the clear value is r, with a share of r-1
        // the vm computes e = x - r
        // then computes r_share + e as the final input sharing
        result.len() == 1 && result[0] == rand_msg.share.share + (x - r)
    }

    // TODO test for failures
}
