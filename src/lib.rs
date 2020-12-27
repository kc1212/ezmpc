mod algebra;
pub mod crypto;
pub mod error;
mod fake_prep;
pub mod message;
pub mod node;
pub mod synchronizer;
pub mod vm;

extern crate auto_ops;
extern crate crossbeam_channel;
extern crate log;
extern crate quick_error;
extern crate rand;

extern crate alga;
extern crate alga_derive;
extern crate num_traits;

#[cfg(test)]
extern crate itertools;
#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;
#[cfg(test)]
extern crate test_env_log;

#[cfg(test)]
mod tests {
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

    const TEST_RNG_SEED: [usize; 4] = [0x0, 0x0, 0x0, 0x0];

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

    fn create_triple_chans(
        n: usize,
        capacity: usize,
    ) -> Vec<(Sender<(AuthShare, AuthShare, AuthShare)>, Receiver<(AuthShare, AuthShare, AuthShare)>)> {
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
        let prog = vec![vm::Instruction::CAdd(2, 1, 0), vm::Instruction::COutput(2), vm::Instruction::Stop];

        let one = Fp::one();
        let two = one + one;

        let fake_alpha_share = Fp::zero();
        let sync_handle = Synchronizer::spawn(sync_chans_for_sync.0, sync_chans_for_sync.1);
        let node_handle = Node::spawn(
            0,
            fake_alpha_share,
            vm::vec_to_reg(&vec![one, one], &vec![]),
            prog,
            sync_chans_for_node.0[0].clone(),
            sync_chans_for_node.1[0].clone(),
            triple_receiver,
            vec![],
            vec![],
            CommitmentScheme::new(),
            TEST_RNG_SEED,
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
            vm::empty_reg(),
            prog,
            sync_chans_for_node.0[0].clone(),
            sync_chans_for_node.1[0].clone(),
            triple_receiver,
            vec![],
            vec![],
            CommitmentScheme::new(),
            TEST_RNG_SEED,
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

        // check for the number of triples in prog and generate enough triples for it
        let triple_count = prog.iter().filter(|i| matches!(i, vm::Instruction::Triple(_, _, _))).count();
        let triple_chans = create_triple_chans(n, triple_count);

        let alpha: Fp = rng.gen();
        let alpha_shares = unauth_share(&alpha, n, rng);

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
                    get_row(&node_chans, i).into_iter().map(|(s, _)| s).collect(),
                    get_col(&node_chans, i).into_iter().map(|(_, r)| r).collect(),
                    CommitmentScheme::new(),
                    TEST_RNG_SEED,
                );
                node_handle
            })
            .collect();

        for _ in 0..triple_count {
            let triple = auth_triple(n, &alpha, rng);
            for (i, (s, _)) in triple_chans.iter().enumerate() {
                s.send((triple.0[i], triple.1[i], triple.2[i])).unwrap();
            }
        }

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

        let rng = &mut StdRng::from_seed(&TEST_RNG_SEED);
        let zero = Fp::zero();
        let regs: Vec<vm::Reg> = transpose(&vec![fake_auth_share(&zero, n, rng)])
            .iter()
            .map(|v| vm::vec_to_reg(&vec![], v))
            .collect();

        generic_integration_test(n, prog, regs, vec![zero], rng);
    }

    #[test]
    fn integration_test_mul() {
        // imagine x is at r0, y is at r1, we use beaver triples to multiply these two numbers
        let n = 3;
        let prog = vec![
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

        let rng = &mut StdRng::from_seed(&TEST_RNG_SEED);
        let x: Fp = rng.gen();
        let y: Fp = rng.gen();
        let expected = x * y;

        let regs: Vec<vm::Reg> = transpose(&vec![fake_auth_share(&x, n, rng), fake_auth_share(&y, n, rng)])
            .iter()
            .map(|v| vm::vec_to_reg(&vec![], v))
            .collect();

        generic_integration_test(n, prog, regs, vec![expected], rng);
    }
}
