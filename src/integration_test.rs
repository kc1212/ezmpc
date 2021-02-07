use crossbeam::channel::{bounded, Receiver, Sender};
use num_traits::{One, Zero};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use std::thread::JoinHandle;
use test_env_log::test;

use crate::algebra::Fp;
use crate::crypto::*;
use crate::message::*;
use crate::party::Party;
use crate::synchronizer::Synchronizer;
use crate::vm::{self, tests::IO_PROG, tests::MUL_PROG};

const TEST_SEED: [u8; 32] = [8u8; 32];
const TEST_CAP: usize = 5;

fn create_sync_chans(
    n: usize,
) -> (
    (Vec<Sender<SyncMsg>>, Vec<Receiver<SyncReplyMsg>>),
    (Vec<Sender<SyncReplyMsg>>, Vec<Receiver<SyncMsg>>),
) {
    let (from_sync, to_party) = (0..n).map(|_| bounded(TEST_CAP)).unzip();
    let (from_party, to_sync) = (0..n).map(|_| bounded(TEST_CAP)).unzip();
    ((from_sync, to_sync), (from_party, to_party))
}

fn create_party_chans(n: usize) -> Vec<Vec<(Sender<PartyMsg>, Receiver<PartyMsg>)>> {
    let mut output = Vec::new();
    for _ in 0..n {
        let mut row = Vec::new();
        for _ in 0..n {
            row.push(bounded(TEST_CAP));
        }
        output.push(row);
    }
    output
}

fn create_chans<T>(n: usize, capacity: usize) -> Vec<(Sender<T>, Receiver<T>)> {
    (0..n).map(|_| bounded(capacity)).collect()
}

fn get_row<T: Clone>(matrix: &Vec<Vec<T>>, row: usize) -> Vec<T> {
    matrix[row].clone()
}

fn get_col<T: Clone>(matrix: &Vec<Vec<T>>, col: usize) -> Vec<T> {
    let mut out = Vec::new();
    for row in matrix {
        out.push(row[col].clone());
    }
    out
}

#[test]
fn integration_test_clear_add() {
    let (sync_chans_for_sync, sync_chans_for_party) = create_sync_chans(1);
    let (_preproc_sender, preproc_receiver) = bounded(TEST_CAP);
    let prog = vec![vm::Instruction::CAdd(2, 1, 0), vm::Instruction::COutput(2), vm::Instruction::Stop];

    let two = Fp::one() + Fp::one();
    let fake_alpha_share = Fp::zero();
    let sync_handle = Synchronizer::spawn(sync_chans_for_sync.0, sync_chans_for_sync.1);
    let party_handle = Party::spawn(
        0,
        fake_alpha_share,
        vm::Reg::from_vec(&vec![Fp::one(), Fp::one()], &vec![]),
        prog,
        sync_chans_for_party.0[0].clone(),
        sync_chans_for_party.1[0].clone(),
        preproc_receiver,
        vec![],
        vec![],
        Some(TEST_SEED),
    );

    let answer = party_handle.join().unwrap().unwrap();
    assert_eq!(answer.len(), 1);
    assert_eq!(answer[0], two);
    assert_eq!((), sync_handle.join().unwrap().unwrap());
}

#[test]
fn integration_test_triple() {
    let (sync_chans_for_sync, sync_chans_for_party) = create_sync_chans(1);
    let (preproc_sender, preproc_receiver) = bounded(TEST_CAP);
    let prog = vec![
        vm::Instruction::Triple(0, 1, 2),
        vm::Instruction::SOutput(0),
        vm::Instruction::SOutput(1),
        vm::Instruction::SOutput(2),
        vm::Instruction::Stop,
    ];

    let zero = AuthShare {
        share: Fp::zero(),
        mac: Fp::zero(),
    };
    let one = AuthShare {
        share: Fp::one(),
        mac: Fp::one(),
    };
    let two = &one + &one;

    preproc_sender.send(PrepMsg::new_triple(zero.clone(), one.clone(), two.clone())).unwrap();

    let fake_alpha_share = Fp::zero();
    let sync_handle = Synchronizer::spawn(sync_chans_for_sync.0, sync_chans_for_sync.1);
    let party_handle = Party::spawn(
        0,
        fake_alpha_share,
        vm::Reg::empty(),
        prog,
        sync_chans_for_party.0[0].clone(),
        sync_chans_for_party.1[0].clone(),
        preproc_receiver,
        vec![],
        vec![],
        Some(TEST_SEED),
    );

    let answer = party_handle.join().unwrap().unwrap();
    assert_eq!(answer.len(), 3);
    assert_eq!(answer[0], zero.share);
    assert_eq!(answer[1], one.share);
    assert_eq!(answer[2], two.share);
    assert_eq!((), sync_handle.join().unwrap().unwrap());
}

fn transpose<T: Clone>(v: &Vec<Vec<T>>) -> Vec<Vec<T>> {
    assert!(!v.is_empty());
    (0..v[0].len())
        .map(|i| v.iter().map(|inner| inner[i].clone()).collect::<Vec<T>>())
        .collect()
}

fn generic_integration_test(n: usize, prog: Vec<vm::Instruction>, regs: Vec<vm::Reg>, expected: Vec<Fp>, rng: &mut impl Rng) {
    let (sync_chans_for_sync, sync_chans_for_party) = create_sync_chans(n);
    let party_chans = create_party_chans(n);

    let alpha: Fp = Fp::random(rng);
    let alpha_shares = unauth_share(&alpha, n, rng);

    // check how many triples and random shares we need and create a preprocessing channel for it
    // TODO this is more rand shares than we need, since we're giving every party max_rand_count number of shares
    let max_rand_count = prog.iter().filter(|i| matches!(i, vm::Instruction::Input(_, _, _))).count();
    let triple_count = prog.iter().filter(|i| matches!(i, vm::Instruction::Triple(_, _, _))).count();
    let preproc_chans = create_chans::<PrepMsg>(n, triple_count + max_rand_count * n);
    let (rand_shares, triples) = gen_fake_prep(n, &alpha, max_rand_count, triple_count, rng);

    for ss in rand_shares {
        for ((chan, _), s) in preproc_chans.iter().zip(ss) {
            chan.send(PrepMsg::RandShare(s)).unwrap();
        }
    }

    for ss in triples {
        for ((chan, _), s) in preproc_chans.iter().zip(ss) {
            chan.send(PrepMsg::Triple(s)).unwrap();
        }
    }

    let sync_handle = Synchronizer::spawn(sync_chans_for_sync.0, sync_chans_for_sync.1);
    // TODO zip auth_shares and regs and iterate
    let party_handles: Vec<JoinHandle<_>> = (0..n)
        .map(|i| {
            let party_handle = Party::spawn(
                i as PartyID,
                alpha_shares[i].clone(),
                regs[i].clone(),
                prog.clone(),
                sync_chans_for_party.0[i].clone(),
                sync_chans_for_party.1[i].clone(),
                preproc_chans[i].1.clone(),
                get_row(&party_chans, i).into_iter().map(|(s, _)| s).collect(),
                get_col(&party_chans, i).into_iter().map(|(_, r)| r).collect(),
                Some(TEST_SEED),
            );
            party_handle
        })
        .collect();

    let mut output_shares = Vec::new();
    for h in party_handles {
        output_shares.push(h.join().unwrap().unwrap());
    }
    assert_eq!(
        expected,
        transpose(&output_shares).iter().map(|shares| unauth_combine(shares)).collect::<Vec<Fp>>()
    );
    assert_eq!((), sync_handle.join().unwrap().unwrap());
}

#[test]
fn integration_test_open() {
    let n = 3;
    let prog = vec![
        vm::Instruction::Input(0, 0, 0),
        vm::Instruction::Open(1, 0),
        vm::Instruction::COutput(1),
        vm::Instruction::Stop,
    ];

    let rng = &mut ChaCha20Rng::from_seed(TEST_SEED);
    let secret = Fp::random(rng);
    let expected = vec![&secret * Fp::from(n)]; // every party outputs the secret, so the expected sum is secret*n
    let regs = vec![vm::Reg::from_vec(&vec![secret], &vec![]), vm::Reg::empty(), vm::Reg::empty()];
    generic_integration_test(n, prog, regs, expected, rng);
}

#[test]
fn integration_test_mul() {
    // imagine x is at r0, y is at r1, we use beaver triples to multiply these two numbers
    let n = 3;

    let rng = &mut ChaCha20Rng::from_seed(TEST_SEED);
    let input_0 = Fp::random(rng);
    let input_1 = Fp::random(rng);
    let expected = vec![&input_0 * &input_1];

    let regs = vec![
        vm::Reg::from_vec(&vec![input_0, Fp::zero()], &vec![]),
        vm::Reg::from_vec(&vec![Fp::zero(), input_1], &vec![]),
        vm::Reg::empty(),
    ];
    generic_integration_test(n, MUL_PROG.to_vec(), regs, expected, rng);
}

#[test]
fn integration_test_input_output() {
    // TODO this test flaky when turning on RUST_LOG=debug and RUST_BACKTRACE=1
    let n = 3;
    let rng = &mut ChaCha20Rng::from_seed(TEST_SEED);
    let input_0 = Fp::random(rng);
    let input_1 = Fp::random(rng);
    let input_2 = Fp::random(rng);
    let expected = vec![input_0.clone(), input_1.clone(), input_2.clone()];
    let regs = vec![
        vm::Reg::from_vec(&vec![input_0, Fp::zero(), Fp::zero()], &vec![]),
        vm::Reg::from_vec(&vec![Fp::zero(), input_1, Fp::zero()], &vec![]),
        vm::Reg::from_vec(&vec![Fp::zero(), Fp::zero(), input_2], &vec![]),
    ];
    generic_integration_test(n, IO_PROG.to_vec(), regs, expected, rng);
}
