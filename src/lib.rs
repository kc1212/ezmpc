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

#[cfg(test)]
extern crate itertools;

#[cfg(test)]
mod tests {

    use crate::crypto::Fp;
    use crate::machine::Machine;
    use crate::message::*;
    use crate::synchronizer::Synchronizer;
    use crossbeam_channel::{bounded, Receiver, Sender};
    use ff::Field;

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
            Inst::COutput,
            Inst::CAdd,
            Inst::CPush(Fp::one()),
            Inst::CPush(Fp::one()),
        ];

        let sync_handle = Synchronizer::spawn(sync_chans.0, sync_chans.1);
        let machine_handle = Machine::spawn(
            machine_chans.0[0].clone(),
            machine_chans.1[0].clone(),
            triple_receiver,
            instructions,
        );

        let answer = machine_handle.join().unwrap().unwrap();
        let two = Fp::one() + Fp::one();
        assert!(answer.1.is_empty());
        assert_eq!(answer.0[0], two);
        assert_eq!((), sync_handle.join().unwrap().unwrap());
    }
}
