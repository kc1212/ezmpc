use crossbeam_channel::{bounded, Receiver, Sender};
use num_traits::{One, Zero};
use rand::{Rng, SeedableRng, StdRng};
use std::thread::JoinHandle;
use test_env_log::test;

use crate::algebra::Fp;
use crate::crypto::*;
use crate::message::*;
use crate::node::Node;
use crate::synchronizer::Synchronizer;
use crate::vm;

const TEST_SEED: [usize; 4] = [0, 1, 2, 3];

fn create_sync_chans(
    n: usize,
) -> (
    (Vec<Sender<SyncMsg>>, Vec<Receiver<SyncMsgReply>>),
    (Vec<Sender<SyncMsgReply>>, Vec<Receiver<SyncMsg>>),
) {
    let (from_sync, to_node) = (0..n).map(|_| bounded(5)).unzip();
    let (from_node, to_sync) = (0..n).map(|_| bounded(5)).unzip();
    ((from_sync, to_sync), (from_node, to_node))
}

fn create_node_chans(n: usize) -> Vec<Vec<(Sender<NodeMsg>, Receiver<NodeMsg>)>> {
    let mut output = Vec::new();
    for _ in 0..n {
        let mut row = Vec::new();
        for _ in 0..n {
            row.push(bounded(5));
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
    let (sync_chans_for_sync, sync_chans_for_node) = create_sync_chans(1);
    let (_triple_sender, triple_receiver) = bounded(5);
    let (_rand_sender, rand_receiver) = bounded(5);
    let prog = vec![vm::Instruction::CAdd(2, 1, 0), vm::Instruction::COutput(2), vm::Instruction::Stop];

    let one = Fp::one();
    let two = one + one;

    let fake_alpha_share = Fp::zero();
    let sync_handle = Synchronizer::spawn(sync_chans_for_sync.0, sync_chans_for_sync.1);
    let node_handle = Node::spawn(
        0,
        fake_alpha_share,
        vm::Reg::from_vec(&vec![one, one], &vec![]),
        prog,
        sync_chans_for_node.0[0].clone(),
        sync_chans_for_node.1[0].clone(),
        triple_receiver,
        rand_receiver,
        vec![],
        vec![],
        commit::Scheme {},
        TEST_SEED,
    );

    let answer = node_handle.join().unwrap().unwrap();
    assert_eq!(answer.len(), 1);
    assert_eq!(answer[0], two);
    assert_eq!((), sync_handle.join().unwrap().unwrap());
}

#[test]
fn integration_test_triple() {
    let (sync_chans_for_sync, sync_chans_for_node) = create_sync_chans(1);
    let (triple_sender, triple_receiver) = bounded(5);
    let (_rand_sender, rand_receiver) = bounded(5);
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
    let two = one + one;
    triple_sender.send((zero, one, two)).unwrap();

    let fake_alpha_share = Fp::zero();
    let sync_handle = Synchronizer::spawn(sync_chans_for_sync.0, sync_chans_for_sync.1);
    let node_handle = Node::spawn(
        0,
        fake_alpha_share,
        vm::Reg::empty(),
        prog,
        sync_chans_for_node.0[0].clone(),
        sync_chans_for_node.1[0].clone(),
        triple_receiver,
        rand_receiver,
        vec![],
        vec![],
        commit::Scheme {},
        TEST_SEED,
    );

    let answer = node_handle.join().unwrap().unwrap();
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
    let (sync_chans_for_sync, sync_chans_for_node) = create_sync_chans(n);
    let node_chans = create_node_chans(n);

    let alpha: Fp = rng.gen();
    let alpha_shares = unauth_share(&alpha, n, rng);

    // check for the number of triples in prog and generate enough triples for it
    let triple_count = prog.iter().filter(|i| matches!(i, vm::Instruction::Triple(_, _, _))).count();
    let triple_chans = create_chans::<(AuthShare, AuthShare, AuthShare)>(n, triple_count);
    for _ in 0..triple_count {
        let triple = auth_triple(n, &alpha, rng);
        for (i, (s, _)) in triple_chans.iter().enumerate() {
            s.send((triple.0[i], triple.1[i], triple.2[i])).unwrap();
        }
    }

    // check for the number of input instructions and generate random shares
    // TODO this is more rand shares than we need, since we're giving every party max_rand_count number of shares
    let max_rand_count = prog.iter().filter(|i| matches!(i, vm::Instruction::Input(_, _, _))).count();
    let rand_chans = create_chans::<InputRandMsg>(n, max_rand_count * n);
    for clear_id in 0..n {
        for _ in 0..max_rand_count {
            let r: Fp = rng.gen();
            let auth_shares = auth_share(&r, n, &alpha, rng);
            let rand_shares: Vec<_> = auth_shares
                .iter()
                .enumerate()
                .map(|(i, share)| InputRandMsg {
                    share: *share,
                    clear: if clear_id == i { Some(r) } else { None },
                    party_id: clear_id,
                })
                .collect();
            for (i, (s, _)) in rand_chans.iter().enumerate() {
                s.send(rand_shares[i]).unwrap();
            }
        }
    }

    let sync_handle = Synchronizer::spawn(sync_chans_for_sync.0, sync_chans_for_sync.1);
    let node_handles: Vec<JoinHandle<_>> = (0..n)
        .map(|i| {
            let node_handle = Node::spawn(
                i,
                alpha_shares[i],
                regs[i],
                prog.clone(),
                sync_chans_for_node.0[i].clone(),
                sync_chans_for_node.1[i].clone(),
                triple_chans[i].1.clone(),
                rand_chans[i].1.clone(),
                get_row(&node_chans, i).into_iter().map(|(s, _)| s).collect(),
                get_col(&node_chans, i).into_iter().map(|(_, r)| r).collect(),
                commit::Scheme {},
                TEST_SEED,
            );
            node_handle
        })
        .collect();

    let mut output_shares = Vec::new();
    for h in node_handles {
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
    let prog = vec![vm::Instruction::Open(1, 0), vm::Instruction::COutput(1), vm::Instruction::Stop];

    //  TODO this function will fail if we do the MAC check at Instruction::Stop too
    let rng = &mut StdRng::from_seed(&TEST_SEED);
    let zero = Fp::zero();
    let regs: Vec<vm::Reg> = transpose(&vec![auth_share(&zero, n, &Fp::zero(), rng)])
        .iter()
        .map(|v| vm::Reg::from_vec(&vec![], v))
        .collect();

    generic_integration_test(n, prog, regs, vec![zero], rng);
}

#[test]
fn integration_test_mul() {
    // imagine x is at r0, y is at r1, we use beaver triples to multiply these two numbers
    let n = 3;
    let prog = vec![
        vm::Instruction::Input(0, 0, 0),     // input [x]
        vm::Instruction::Input(1, 1, 1),     // input [y]
        vm::Instruction::Triple(2, 3, 4),    // [a], [b], [c]
        vm::Instruction::SSub(5, 0, 2),      // [e] <- [x] - [a]
        vm::Instruction::SSub(6, 1, 3),      // [d] <- [y] - [b]
        vm::Instruction::Open(5, 5),         // e <- open [e]
        vm::Instruction::Open(6, 6),         // d <- open [d]
        vm::Instruction::MMul(7, 3, 5),      // [b] * e
        vm::Instruction::MMul(8, 2, 6),      // d * [a]
        vm::Instruction::CMul(9, 5, 6),      // e*d
        vm::Instruction::SAdd(10, 4, 7),     // [c] + [e*b]
        vm::Instruction::SAdd(10, 10, 8),    //     + [d*a]
        vm::Instruction::MAdd(10, 10, 9, 0), //     + e*d
        vm::Instruction::SOutput(10),
        vm::Instruction::Stop,
    ];

    let rng = &mut StdRng::from_seed(&TEST_SEED);
    let input_0: Fp = rng.gen();
    let input_1: Fp = rng.gen();
    let expected = vec![input_0 * input_1];

    let regs = vec![
        vm::Reg::from_vec(&vec![input_0, Fp::zero()], &vec![]),
        vm::Reg::from_vec(&vec![Fp::zero(), input_1], &vec![]),
        vm::Reg::empty(),
    ];
    generic_integration_test(n, prog, regs, expected, rng);
}

#[test]
fn integration_test_input_output() {
    let n = 3;
    let prog = vec![
        vm::Instruction::Input(0, 0, 0),
        vm::Instruction::Input(1, 1, 1),
        vm::Instruction::Input(2, 2, 2),
        vm::Instruction::COutput(0),
        vm::Instruction::COutput(1),
        vm::Instruction::SOutput(2),
        vm::Instruction::Stop,
    ];

    let rng = &mut StdRng::from_seed(&TEST_SEED);
    let input_0: Fp = rng.gen();
    let input_1: Fp = rng.gen();
    let input_2: Fp = rng.gen();
    let expected = vec![input_0, input_1, input_2];
    let regs = vec![
        vm::Reg::from_vec(&vec![input_0, Fp::zero(), Fp::zero()], &vec![]),
        vm::Reg::from_vec(&vec![Fp::zero(), input_1, Fp::zero()], &vec![]),
        vm::Reg::from_vec(&vec![Fp::zero(), Fp::zero(), input_2], &vec![]),
    ];
    generic_integration_test(n, prog, regs, expected, rng);
}
