use crate::algebra::Fp;
use crate::crypto::AuthShare;
use crate::error::{MPCError, OutputError, TIMEOUT};
use crate::message::{InputRandMsg, PartyID};

use crossbeam_channel::{bounded, select, Receiver, Sender};
use std::cmp::min;
use std::collections::HashMap;
use std::ops;
use std::thread;
use std::thread::JoinHandle;

type RegAddr = usize;

/// Reg is the register stored by the VM.
#[derive(Copy, Clone, Debug)]
pub struct Reg {
    clear: [Option<Fp>; REG_SIZE],
    secret: [Option<AuthShare>; REG_SIZE],
}

impl Reg {
    /// Construct an empty register.
    pub fn empty() -> Reg {
        Reg {
            clear: [None; REG_SIZE],
            secret: [None; REG_SIZE],
        }
    }

    /// Construct a register from a vector of clear values and authenticated secret shares.
    pub fn from_vec(vclear: &Vec<Fp>, vsecret: &Vec<AuthShare>) -> Reg {
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
}

const REG_SIZE: usize = 128;

/// The stateful virtual machine that execute instructions defined in `Instruction`.
/// It communicates with the outside world using channels if it needs additional information.
pub struct VM {
    id: PartyID,
    alpha_share: Fp, // could be a reference type
    reg: Reg,
    triple_chan: Receiver<(AuthShare, AuthShare, AuthShare)>,
    rand_chan: Receiver<InputRandMsg>,
    rand_msgs: HashMap<PartyID, Vec<InputRandMsg>>,
    partial_openings: Vec<(Fp, AuthShare)>,
}

/// These are the possible action items that the VM cannot handle by itself.
#[derive(Clone, Debug)]
pub enum Action {
    /// Ask for the next instruction.
    Next,
    /// Partially open the share.
    Open(Fp, Sender<Fp>),
    /// Secret share an input.
    Input(PartyID, Option<Fp>, Sender<Fp>),
    /// Perform the MAC check.
    Check(Vec<(Fp, AuthShare)>, Sender<Result<(), OutputError>>),
}

/// These are the instructions for the VM.
/// We use the prefix `C`, `S` and `M` to denote different types of operation.
/// * `C` (e.g. `CAdd`): These operate on clear registers.
/// * `S` (e.g. `SAdd`): These operate on secret registers.
/// * `M` (e.g. `MAdd`): These operate on secret and clear registers.
/// Most instructions store the result in the register of the first operand.
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

fn opt_to_res<T>(v: Option<T>) -> Result<T, MPCError> {
    match v {
        Some(x) => Ok(x),
        None => Err(MPCError::EmptyError),
    }
}

impl VM {
    /// Spawns a new VM thread and returns its handler.
    /// This function assumes all the VMs have a unique `id`,
    /// the global MAC key share (`alpha_share`) is correct and that
    /// the channels are not disconnected before calling `.join` on the returned handler.
    pub fn spawn(
        id: PartyID,
        alpha_share: Fp,
        reg: Reg,
        triple_chan: Receiver<(AuthShare, AuthShare, AuthShare)>,
        rand_chan: Receiver<InputRandMsg>,
        r_chan: Receiver<Instruction>,
        s_chan: Sender<Action>,
    ) -> JoinHandle<Result<Vec<Fp>, MPCError>> {
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
    fn listen(&mut self, r_chan: Receiver<Instruction>, s_chan: Sender<Action>) -> Result<Vec<Fp>, MPCError> {
        let mut output = Vec::new();

        loop {
            let inst = r_chan.recv_timeout(TIMEOUT)?;
            match inst {
                Instruction::CAdd(r0, r1, r2) => self.do_clear_op(r0, r1, r2, ops::Add::add)?,
                Instruction::CSub(r0, r1, r2) => self.do_clear_op(r0, r1, r2, ops::Sub::sub)?,
                Instruction::CMul(r0, r1, r2) => self.do_clear_op(r0, r1, r2, ops::Mul::mul)?,
                Instruction::SAdd(r0, r1, r2) => self.do_secret_op(r0, r1, r2, ops::Add::add)?,
                Instruction::SSub(r0, r1, r2) => self.do_secret_op(r0, r1, r2, ops::Sub::sub)?,
                Instruction::MAdd(r0, r1, r2, id) => self.do_mixed_add(r0, r1, r2, id)?,
                Instruction::MMul(r0, r1, r2) => self.do_mixed_mul(r0, r1, r2)?,
                Instruction::Input(r0, r1, id) => self.do_input(r0, r1, id, &s_chan)?,
                Instruction::Triple(r0, r1, r2) => self.do_triple(r0, r1, r2)?,
                Instruction::Open(to, from) => self.do_open(to, from, &s_chan)?,
                Instruction::COutput(reg) => output.push(opt_to_res(self.reg.clear[reg])?),
                Instruction::SOutput(reg) => {
                    let result = self.do_secret_output(reg, &s_chan)?;
                    output.push(result);
                }
                Instruction::Stop => {
                    if !self.partial_openings.is_empty() {
                        self.do_mac_check(&s_chan)?;
                    }
                    // need to send a next before we return to say we're done with this instruction
                    s_chan.send(Action::Next)?;
                    return Ok(output);
                }
            }
            s_chan.send(Action::Next)?;
        }
    }

    fn do_clear_op<F>(&mut self, r0: RegAddr, r1: RegAddr, r2: RegAddr, op: F) -> Result<(), MPCError>
    where
        F: Fn(Fp, Fp) -> Fp,
    {
        let c = self.reg.clear[r1].zip(self.reg.clear[r2]).map(|(a, b)| op(a, b));
        self.reg.clear[r0] = Some(opt_to_res(c)?);
        Ok(())
    }

    fn do_secret_op<F>(&mut self, r0: RegAddr, r1: RegAddr, r2: RegAddr, op: F) -> Result<(), MPCError>
    where
        F: Fn(AuthShare, AuthShare) -> AuthShare,
    {
        let c = self.reg.secret[r1].zip(self.reg.secret[r2]).map(|(a, b)| op(a, b));
        self.reg.secret[r0] = Some(opt_to_res(c)?);
        Ok(())
    }

    fn do_mixed_add(&mut self, s_r0: RegAddr, s_r1: RegAddr, c_r2: RegAddr, id: PartyID) -> Result<(), MPCError> {
        let c = self.reg.secret[s_r1]
            .zip(self.reg.clear[c_r2])
            .map(|(a, b)| a.add_clear(&b, &self.alpha_share, self.id == id));
        self.reg.secret[s_r0] = Some(opt_to_res(c)?);
        Ok(())
    }

    fn do_mixed_mul(&mut self, s_r0: RegAddr, s_r1: RegAddr, c_r2: RegAddr) -> Result<(), MPCError> {
        let c = self.reg.secret[s_r1].zip(self.reg.clear[c_r2]).map(|(a, b)| a.mul_clear(&b));
        self.reg.secret[s_r0] = Some(opt_to_res(c)?);
        Ok(())
    }

    fn get_rand_share_for_id(&mut self, id: PartyID) -> Result<InputRandMsg, MPCError> {
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

    fn do_input(&mut self, r0: RegAddr, r1: RegAddr, id: PartyID, s_chan: &Sender<Action>) -> Result<(), MPCError> {
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
        let input_share = rand_share.share.add_clear(&e, &self.alpha_share, self.id == id);
        self.reg.secret[r0] = Some(input_share);
        Ok(())
    }

    fn do_triple(&mut self, r0: RegAddr, r1: RegAddr, r2: RegAddr) -> Result<(), MPCError> {
        let triple = self.triple_chan.recv_timeout(TIMEOUT)?;
        self.reg.secret[r0] = Some(triple.0);
        self.reg.secret[r1] = Some(triple.1);
        self.reg.secret[r2] = Some(triple.2);
        Ok(())
    }

    fn do_open(&mut self, to: RegAddr, from: RegAddr, s_chan: &Sender<Action>) -> Result<(), MPCError> {
        match self.reg.secret[from] {
            None => Err(MPCError::EmptyError),
            Some(for_opening) => {
                let (s, r) = bounded(1);
                s_chan.send(Action::Open(for_opening.share, s))?;

                // wait for the response
                let opened: Fp = r.recv_timeout(TIMEOUT)?;
                self.reg.clear[to] = Some(opened);

                // store the opened value for mac_check later
                self.partial_openings.push((opened, for_opening));
                Ok(())
            }
        }
    }

    fn do_secret_output(&mut self, reg: RegAddr, s_chan: &Sender<Action>) -> Result<Fp, MPCError> {
        // first do the open step, just like process_open, but don't store the value
        let share = match self.reg.secret[reg] {
            None => Err(OutputError::RegisterEmpty),
            Some(x) => {
                let (s, r) = bounded(1);
                s_chan.send(Action::Open(x.share, s))?;
                let opened: Fp = r.recv_timeout(TIMEOUT)?;

                self.partial_openings.push((opened, x));
                Ok(x)
            }
        }?;

        self.do_mac_check(s_chan)?;
        Ok(share.share)
    }

    fn do_mac_check(&mut self, s_chan: &Sender<Action>) -> Result<(), MPCError> {
        // next do the mac_check
        let (s, r) = bounded(5);
        s_chan.send(Action::Check(self.partial_openings.clone(), s))?;

        // wait for response and clear the partial opening vector
        r.recv_timeout(TIMEOUT)??;
        self.partial_openings.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::Zero;

    fn unauth_vec_to_reg(vclear: &Vec<Fp>, vsecret: &Vec<Fp>) -> Reg {
        let vv: Vec<_> = vsecret
            .iter()
            .map(|x| AuthShare {
                share: *x,
                mac: Zero::zero(),
            })
            .collect();
        Reg::from_vec(vclear, &vv)
    }

    fn simple_vm_runner(prog: Vec<Instruction>, reg: Reg) -> Result<Vec<Fp>, MPCError> {
        let (_, dummy_triple_chan) = bounded(5);
        let (_, dummy_rand_chan) = bounded(5);
        vm_runner(prog, reg, dummy_triple_chan, dummy_rand_chan)
    }

    // TODO return additional information for testing, e.g., how many MAC check we did
    fn vm_runner(
        prog: Vec<Instruction>,
        reg: Reg,
        triple_chan: Receiver<(AuthShare, AuthShare, AuthShare)>,
        rand_chan: Receiver<InputRandMsg>,
    ) -> Result<Vec<Fp>, MPCError> {
        let (s_instruction_chan, r_instruction_chan) = bounded(5);
        let (s_action_chan, r_action_chan) = bounded(5);

        let fake_alpha_share = Fp::zero();
        let handle = VM::spawn(0, fake_alpha_share, reg, triple_chan, rand_chan, r_instruction_chan, s_action_chan);
        for instruction in prog {
            s_instruction_chan.send(instruction)?;

            loop {
                // these replies are obviously not the correct implementation, they're only here for testing
                // the actual implementation is in node.rs
                let reply = r_action_chan.recv_timeout(TIMEOUT)?;
                match reply {
                    Action::Next => {
                        break;
                    }
                    Action::Open(x, sender) => sender.send(x)?,
                    Action::Input(_, e_option, sender) => match e_option {
                        Some(e) => sender.send(e)?,
                        None => sender.send(Fp::zero())?,
                    },
                    Action::Check(_, sender) => sender.send(Ok(()))?,
                }
            }

            if instruction == Instruction::Stop {
                break;
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
        let reg = Reg::from_vec(&vec![a, b], &vec![]);
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
    fn prop_open(s: Fp) -> bool {
        let prog = vec![Instruction::Open(0, 0), Instruction::COutput(0), Instruction::Stop];
        let reg = unauth_vec_to_reg(&vec![], &vec![s]);

        let result = simple_vm_runner(prog, reg).unwrap();

        // the result should be whatever is in the register since the simple_vm_runner just does an echo
        result.len() == 1 && result[0] == s
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

        let a_share = AuthShare { share: a, mac: Fp::zero() };
        let b_share = AuthShare { share: b, mac: Fp::zero() };
        let c_share = AuthShare { share: c, mac: Fp::zero() };
        s_triple_chan.send((a_share, b_share, c_share)).unwrap();
        let result = vm_runner(prog, Reg::empty(), r_triple_chan, dummy_rand_chan).unwrap();
        result.len() == 3 && result[0] == a_share.share && result[1] == b_share.share && result[2] == c_share.share
    }

    #[quickcheck]
    fn prop_input(r: Fp, r_share: Fp, x: Fp) -> bool {
        let prog = vec![Instruction::Input(0, 0, 0), Instruction::SOutput(0), Instruction::Stop];

        let (_, dummy_triple_chan) = bounded(5);
        let (s_rand_chan, r_rand_chan) = bounded(5);

        let rand_msg = InputRandMsg {
            share: AuthShare {
                share: r_share,
                mac: Fp::zero(),
            },
            clear: Some(r),
            party_id: 0,
        };
        s_rand_chan.send(rand_msg).unwrap();
        let result = vm_runner(prog, unauth_vec_to_reg(&vec![x], &vec![]), dummy_triple_chan, r_rand_chan).unwrap();

        // for rand_msg, the clear value is r, with a share of r-1
        // the vm computes e = x - r
        // then computes r_share + e as the final input sharing
        result.len() == 1 && result[0] == rand_msg.share.share + (x - r)
    }

    // TODO test for failures
}
