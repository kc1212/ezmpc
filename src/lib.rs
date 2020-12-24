mod crypto;
mod error;
mod node;
mod message;
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

    use crate::crypto::Fp;
    use crate::node::Node;
    use crate::message::*;
    use crate::synchronizer::Synchronizer;
    use crossbeam_channel::{bounded, Receiver, Sender};
    use ff::Field;
    use crate::vm;
    use test_env_log::test;

    fn create_chans(
        n: usize,
    ) -> (
        (Vec<Sender<SyncMsg>>, Vec<Receiver<SyncMsgReply>>),
        (Vec<Sender<SyncMsgReply>>, Vec<Receiver<SyncMsg>>),
    ) {
        let (from_sync, to_node): (Vec<_>, Vec<_>) = vec![bounded(5); n].iter().cloned().unzip();
        let (from_node, to_sync): (Vec<_>, Vec<_>) = vec![bounded(5); n].iter().cloned().unzip();
        ((from_sync, to_sync), (from_node, to_node))
    }

    #[test]
    fn integration_test_sync() {
        let (sync_chans, node_chans) = create_chans(1);
        let (_triple_sender, triple_receiver) = bounded(5);
        let prog= vec![
            vm::Instruction::ADD(2, 1, 0),
            vm::Instruction::OUTPUT(2),
            vm::Instruction::STOP,
        ];

        let one = Fp::one();
        let two = one + one;

        let sync_handle = Synchronizer::spawn(sync_chans.0, sync_chans.1);
        let node_handle = Node::spawn(
            0,
            node_chans.0[0].clone(),
            node_chans.1[0].clone(),
            triple_receiver,
            vec![],
            vec![],
            prog,
            vm::vec_to_reg(&vec![one, one]),
        );

        let answer = node_handle.join().unwrap().unwrap();
        assert_eq!(answer.len() , 1);
        assert_eq!(answer[0], two);
        assert_eq!((), sync_handle.join().unwrap().unwrap());
    }
    
    #[test]
    fn integration_test_triple() {
        let (sync_chans, node_chans) = create_chans(1);
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

        let sync_handle = Synchronizer::spawn(sync_chans.0, sync_chans.1);
        let node_handle = Node::spawn(
            0,
            node_chans.0[0].clone(),
            node_chans.1[0].clone(),
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
}
