//! This module implement a party that participates in the MPC protocol.
//! It assumes perfect channels for sending and receiving messages.
//! The actual networking layer is handled by an outer layer.

use crate::algebra::Fp;
use crate::crypto::commit;
use crate::crypto::AuthShare;
use crate::error::{MACCheckError, MPCError, TIMEOUT};
use crate::message;
use crate::message::{PartyID, PartyMsg, PreprocMsg, SyncMsg, SyncReplyMsg};
use crate::vm;

use crossbeam::channel::{bounded, select, Receiver, Sender};
use log::{debug, error};
use num_traits::Zero;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use std::thread;

const FORWARDING_CAP: usize = 1024;

pub struct Party {
    id: PartyID,
    alpha_share: Fp,
    com_scheme: commit::Scheme,
    s_sync_chan: Sender<SyncReplyMsg>,
    r_sync_chan: Receiver<SyncMsg>,
    preproc_chan: Receiver<PreprocMsg>,
    s_party_chans: Vec<Sender<PartyMsg>>,
    r_party_chans: Vec<Receiver<PartyMsg>>,
}

impl Party {
    /// Spawn a party thread and returns a handler.
    /// If successful, the handler will return the result of the computation,
    /// i.e., the result of calling `COutput` or `SOutput`.
    pub fn spawn(
        id: PartyID,
        alpha_share: Fp,
        reg: vm::Reg,
        prog: Vec<vm::Instruction>,
        s_sync_chan: Sender<SyncReplyMsg>,
        r_sync_chan: Receiver<SyncMsg>,
        preproc_chan: Receiver<PreprocMsg>,
        s_party_chan: Vec<Sender<PartyMsg>>,
        r_party_chan: Vec<Receiver<PartyMsg>>,
        rng_seed: [u8; 32],
    ) -> thread::JoinHandle<Result<Vec<Fp>, MPCError>> {
        thread::spawn(move || {
            let p = Party {
                id,
                alpha_share,
                com_scheme: commit::Scheme {},
                s_sync_chan,
                r_sync_chan,
                preproc_chan,
                s_party_chans: s_party_chan,
                r_party_chans: r_party_chan,
            };
            p.listen(reg, prog, rng_seed)
        })
    }

    fn listen(&self, reg: vm::Reg, prog: Vec<vm::Instruction>, rng_seed: [u8; 32]) -> Result<Vec<Fp>, MPCError> {
        let rng = &mut ChaCha20Rng::from_seed(rng_seed);

        // init forwarding channels
        let (s_inner_triple_chan, r_inner_triple_chan) = bounded(FORWARDING_CAP);
        let (s_inner_rand_chan, r_inner_rand_chan) = bounded(FORWARDING_CAP);

        // start the vm
        let (s_inst_chan, r_inst_chan) = bounded(vm::DEFAULT_CAP);
        let (s_action_chan, r_action_chan) = bounded(vm::DEFAULT_CAP);
        let vm_handler: thread::JoinHandle<_> = vm::VM::spawn(
            self.id,
            self.alpha_share.clone(),
            reg,
            r_inner_triple_chan,
            r_inner_rand_chan,
            r_inst_chan,
            s_action_chan,
        );
        let mut pc = 0;

        // wait for start, collect the preprocessing message while we wait
        loop {
            select! {
                recv(self.r_sync_chan) -> msg_res => {
                    let msg = msg_res?;
                    if msg == SyncMsg::Start {
                        debug!("[{}] Starting", self.id);
                        break;
                    } else {
                        debug!("[{}] Received {:?} while waiting to start", self.id, msg);
                    }
                }
                recv(self.preproc_chan) -> x => {
                    debug!("[{}] got preproc msg {:?}", self.id, x);
                    match x? {
                        PreprocMsg::Triple(msg) => {
                            s_inner_triple_chan.try_send(msg)?
                        }
                        PreprocMsg::RandShare(msg) => {
                            s_inner_rand_chan.try_send(msg)?
                        }
                    }
                }
            }
        }

        // process instructions
        loop {
            select! {
                recv(self.preproc_chan) -> x => {
                    debug!("[{}] got preproc msg {:?}", self.id, x);
                    match x? {
                        PreprocMsg::Triple(msg) => {
                            s_inner_triple_chan.try_send(msg)?
                        }
                        PreprocMsg::RandShare(msg) => {
                            s_inner_rand_chan.try_send(msg)?
                        }
                    }
                }
                recv(self.r_sync_chan) -> v => {
                    let msg: SyncMsg = v?;
                    match msg {
                        SyncMsg::Start => panic!("party already started"),
                        SyncMsg::Next => {
                            if pc >= prog.len() {
                                panic!("instruction counter overflow");
                            }
                            let instruction = prog[pc].clone();
                            pc += 1;

                            debug!("[{}] Sending instruction {:?} to VM", self.id, instruction);
                            s_inst_chan.send(instruction.clone())?;
                            // NOTE there's a bug here because this function blocks,
                            // which means we cannot forward preprocessing data to the VM.
                            // then if the VM asks for more triples/rand shares when there's
                            // nothing in the channel buffer then the program crashes
                            self.handle_vm_actions(&r_action_chan, rng)?;

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

    fn bcast(&self, m: PartyMsg) -> Result<(), MPCError> {
        message::broadcast(&self.s_party_chans, m)?;
        Ok(())
    }

    fn recv(&self) -> Result<Vec<PartyMsg>, MPCError> {
        let out = message::receive(&self.r_party_chans, TIMEOUT)?;
        Ok(out)
    }

    fn mac_check(&self, x: &Fp, share: &AuthShare, rng: &mut impl Rng) -> Result<Result<(), MACCheckError>, MPCError> {
        // let d = alpha_i * x - mac_i
        let d = &self.alpha_share * x - &share.mac;
        // commit d
        let (d_com, d_open) = self.com_scheme.commit(d, rng);
        self.bcast(PartyMsg::Com(d_com))?;
        // get commitment from others
        let d_coms: Vec<_> = self.recv()?.into_iter().map(|x| x.unwrap_com()).collect();
        // commit-open d and collect them
        self.bcast(PartyMsg::Opening(d_open))?;
        let d_opens: Vec<_> = self.recv()?.into_iter().map(|x| x.unwrap_opening()).collect();
        // verify all the commitments of d
        // and check they sum to 0
        let coms_ok = d_opens.iter().zip(d_coms).map(|(o, c)| self.com_scheme.verify(&o, &c)).all(|x| x);
        let zero_ok = d_opens.into_iter().map(|o| o.get_v()).sum::<Fp>() == Fp::zero();

        // this is a weird kind of return type but it makes categorizing the errors easier
        if !coms_ok {
            Ok(Err(MACCheckError::BadCommitment))
        } else if !zero_ok {
            Ok(Err(MACCheckError::SumIsNotZero))
        } else {
            Ok(Ok(()))
        }
    }

    fn handle_vm_actions(&self, r_action_chan: &Receiver<vm::Action>, rng: &mut impl Rng) -> Result<(), MPCError> {
        loop {
            let action = r_action_chan.recv_timeout(TIMEOUT)?;
            debug!("[{}], Received action {:?} from VM", self.id, action);
            match action {
                vm::Action::Next => {
                    break;
                }
                vm::Action::Open(x, sender) => {
                    self.bcast(PartyMsg::Elem(x))?;
                    let result = self.recv()?.into_iter().map(|x| x.unwrap_elem()).sum();
                    debug!("[{}] Partially opened {:?}", self.id, result);
                    sender.send(result)?
                }
                vm::Action::Input(id, e_option, sender) => {
                    match e_option {
                        Some(e) => self.bcast(PartyMsg::Elem(e))?,
                        None => (),
                    };
                    let e = self.r_party_chans[id].recv_timeout(TIMEOUT)?.unwrap_elem();
                    sender.send(e)?
                }
                vm::Action::Check(openings, sender) => {
                    // mac_check everything and send error on first failure
                    let mut ok = true;
                    for (x, opening) in openings {
                        match self.mac_check(&x, &opening, rng)? {
                            Ok(()) => {}
                            e => {
                                error!("[{}] MAC check failed: {:?}", self.id, e);
                                sender.send(e)?;
                                ok = false;
                                break;
                            }
                        }
                    }

                    if ok {
                        debug!("[{}] All MAC check ok", self.id);
                        sender.send(Ok(()))?;
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
    use crate::crypto::{auth_share, unauth_share};

    const TEST_SEED: [u8; 32] = [8u8; 32];
    const TEST_CAP: usize = 5;

    fn make_dummy_party(alpha_share: Fp, s_party_chans: Vec<Sender<PartyMsg>>, r_party_chans: Vec<Receiver<PartyMsg>>) -> Party {
        let (dummy_s_sync_chan, _) = bounded(TEST_CAP);
        let (_, dummy_r_sync_chan) = bounded(TEST_CAP);
        let (_, dummy_preproc_chan) = bounded(TEST_CAP);
        Party {
            id: 0,
            alpha_share,
            com_scheme: commit::Scheme {},
            s_sync_chan: dummy_s_sync_chan,
            r_sync_chan: dummy_r_sync_chan,
            preproc_chan: dummy_preproc_chan,
            s_party_chans,
            r_party_chans,
        }
    }

    #[test]
    fn test_mac_check() {
        let n = 2;
        let rng = &mut ChaCha20Rng::from_seed(TEST_SEED);
        let alpha = Fp::random(rng);
        let alpha_shares = unauth_share(&alpha, n, rng);

        // note:
        // chan0 is for echoing
        // chan1 is a black hole
        // chan2 is for sending messages to the party from the test
        let (s_party_chan0, r_party_chan0) = bounded(TEST_CAP);
        let (s_party_chan1, _r_party_chan1) = bounded(TEST_CAP);
        let (s_party_chan2, r_party_chan2) = bounded(TEST_CAP);
        let party = make_dummy_party(
            alpha_shares[0].clone(),
            vec![s_party_chan0, s_party_chan1],
            vec![r_party_chan0, r_party_chan2],
        );

        let x = Fp::random(rng);
        let x_shares = auth_share(&x, n, &alpha, rng);

        // use the wrong commitment
        {
            // receive a commitment from party and send a commitment
            let d = &alpha_shares[1] * &x - &x_shares[1].mac;
            let (commitment, _) = party.com_scheme.commit(d.clone(), rng);
            s_party_chan2.send(PartyMsg::Com(commitment)).unwrap();

            // get opening from party and send the *bad* opening
            let (_, bad_opening) = party.com_scheme.commit(d, rng);
            s_party_chan2.send(PartyMsg::Opening(bad_opening)).unwrap();

            // party should fail with bad commitment
            let result = party.mac_check(&x, &x_shares[0], rng).unwrap();
            assert_eq!(result.unwrap_err(), MACCheckError::BadCommitment);

            // empty the black hole
            _r_party_chan1.recv().unwrap();
            _r_party_chan1.recv().unwrap();
        }

        // use the wrong x so that the opening is not 0
        {
            let bad_alpha = Fp::random(rng);
            let x_shares_2 = auth_share(&x, n, &bad_alpha, rng);

            // receive a commitment from party and send a commitment
            let d = &alpha_shares[1] * &x - &x_shares_2[1].mac;
            let (commitment, opening) = party.com_scheme.commit(d.clone(), rng);
            s_party_chan2.send(PartyMsg::Com(commitment)).unwrap();

            // get opening from party and send the opening
            s_party_chan2.send(PartyMsg::Opening(opening)).unwrap();

            // party should fail with sum-not-zero since we use a bad alpha
            let result = party.mac_check(&x, &x_shares_2[0], rng).unwrap();
            assert_eq!(result.unwrap_err(), MACCheckError::SumIsNotZero);

            // empty the black hole
            _r_party_chan1.recv().unwrap();
            _r_party_chan1.recv().unwrap();
        }

        // everything ok
        {
            // receive a commitment from party and send a commitment
            let d = &alpha_shares[1] * &x - &x_shares[1].mac;
            let (commitment, opening) = party.com_scheme.commit(d.clone(), rng);
            s_party_chan2.send(PartyMsg::Com(commitment)).unwrap();

            // get opening from party and send the opening
            s_party_chan2.send(PartyMsg::Opening(opening)).unwrap();

            // everything should be ok
            let result = party.mac_check(&x, &x_shares[0], rng).unwrap();
            assert_eq!(result.unwrap(), ());

            // empty the black hole
            _r_party_chan1.recv().unwrap();
            _r_party_chan1.recv().unwrap();
        }
    }
}
