mod crypto;
mod error;
mod message;
mod node;
mod synchronizer;
mod vm;

extern crate crossbeam_channel;
extern crate ff;
extern crate rand;
#[macro_use]
extern crate quick_error;
extern crate log;

#[cfg(test)]
extern crate itertools;
#[cfg(test)]
extern crate test_env_log;

#[cfg(test)]
mod tests {

    use std::thread::JoinHandle;

    use crate::crypto::{combine, share, Fp};
    use crate::message::*;
    use crate::node::Node;
    use crate::synchronizer::Synchronizer;
    use crate::vm;
    use crossbeam_channel::{bounded, Receiver, Sender};
    use ff::Field;
    use rand::{SeedableRng, XorShiftRng};
    const SEED: [u32; 4] = [0x5dbe6259, 0x8d313d76, 0x3237db17, 0xe5bc0654];
    use test_env_log::test;

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

    fn create_node_chans(n: usize) -> Vec<Vec<(Sender<Fp>, Receiver<Fp>)>> {
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
    fn integration_test_sync() {
        let (sync_chans_for_sync, sync_chans_for_node) = create_sync_chans(1);
        let (_triple_sender, triple_receiver) = bounded(5);
        let prog = vec![
            vm::Instruction::ADD(2, 1, 0),
            vm::Instruction::OUTPUT(2),
            vm::Instruction::STOP,
        ];

        let one = Fp::one();
        let two = one + one;

        let sync_handle = Synchronizer::spawn(sync_chans_for_sync.0, sync_chans_for_sync.1);
        let node_handle = Node::spawn(
            0,
            sync_chans_for_node.0[0].clone(),
            sync_chans_for_node.1[0].clone(),
            triple_receiver,
            vec![],
            vec![],
            prog,
            vm::vec_to_reg(&vec![one, one]),
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
            vm::Instruction::TRIPLE(0, 1, 2),
            vm::Instruction::OUTPUT(0),
            vm::Instruction::OUTPUT(1),
            vm::Instruction::OUTPUT(2),
            vm::Instruction::STOP,
        ];

        let zero = Fp::zero();
        let one = Fp::one();
        let two = one + one;
        triple_sender.send((zero, one, two)).unwrap();

        let sync_handle = Synchronizer::spawn(sync_chans_for_sync.0, sync_chans_for_sync.1);
        let node_handle = Node::spawn(
            0,
            sync_chans_for_node.0[0].clone(),
            sync_chans_for_node.1[0].clone(),
            triple_receiver,
            vec![],
            vec![],
            prog,
            vm::empty_reg(),
        );

        let answer = node_handle.join().unwrap().unwrap();
        assert_eq!(answer.len(), 3);
        assert_eq!(answer[0], zero);
        assert_eq!(answer[1], one);
        assert_eq!(answer[2], two);
        assert_eq!((), sync_handle.join().unwrap().unwrap());
    }

    #[test]
    fn integration_test_open() {
        let n = 2;
        let (sync_chans_for_sync, sync_chans_for_node) = create_sync_chans(n);
        let node_chans = create_node_chans(n);
        let (_triple_sender, triple_receiver) = bounded(5);
        let prog = vec![
            vm::Instruction::OPEN(0, 0),
            vm::Instruction::OUTPUT(0),
            vm::Instruction::STOP,
        ];

        let rng = &mut XorShiftRng::from_seed(SEED);
        let zero = Fp::zero();
        let zero_boxes = share(&zero, n, rng);

        let sync_handle = Synchronizer::spawn(sync_chans_for_sync.0, sync_chans_for_sync.1);
        let node_handles: Vec<JoinHandle<_>> = (0..n)
            .map(move |i| {
                let node_handle = Node::spawn(
                    i,
                    sync_chans_for_node.0[i].clone(),
                    sync_chans_for_node.1[i].clone(),
                    triple_receiver.clone(),
                    get_row(&node_chans, i)
                        .iter()
                        .map(|(s, _)| s.clone())
                        .collect(),
                    get_col(&node_chans, i)
                        .iter()
                        .map(|(_, r)| r.clone())
                        .collect(),
                    prog.clone(),
                    vm::vec_to_reg(&vec![zero_boxes[i]]),
                );
                node_handle
            })
            .collect();

        let mut output = Vec::new();
        for h in node_handles {
            output.push(h.join().unwrap().unwrap()[0]);
        }
        assert_eq!(zero, combine(&output));
        assert_eq!((), sync_handle.join().unwrap().unwrap());
    }
}
