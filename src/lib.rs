mod crypto;
mod error;
mod machine;
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
    use crate::machine::Machine;
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
        let (from_sync, to_machine): (Vec<_>, Vec<_>) = vec![bounded(5); n].iter().cloned().unzip();
        let (from_machine, to_sync): (Vec<_>, Vec<_>) = vec![bounded(5); n].iter().cloned().unzip();
        ((from_sync, to_sync), (from_machine, to_machine))
    }

    #[test]
    fn test_sync() {
        let (sync_chans, machine_chans) = create_chans(1);
        let (_, triple_receiver) = bounded(5);
        let instructions = vec![
            vm::Instruction::ADD(2, 1, 0),
            vm::Instruction::OUTPUT(2),
            vm::Instruction::STOP,
        ];

        let one = Fp::one();
        let two = one + one;
        let sync_handle = Synchronizer::spawn(sync_chans.0, sync_chans.1);
        let machine_handle = Machine::spawn(
            0,
            machine_chans.0[0].clone(),
            machine_chans.1[0].clone(),
            triple_receiver,
            instructions,
            vm::vec_to_reg(&vec![one, one]),
        );

        let answer = machine_handle.join().unwrap().unwrap();
        assert_eq!(answer.len() , 1);
        assert_eq!(answer[0], two);
        assert_eq!((), sync_handle.join().unwrap().unwrap());
    }
}
