use crate::crypto::*;
use crate::vm::PartyID;
use rand::Rng;

struct FakePrep {
    n: usize,
    alpha: Fp,
}

impl FakePrep {
    fn gen_rand(&self, id: PartyID, rng: &mut impl Rng) -> Vec<AuthRand> {
        let r: Fp = rng.gen();
        let r_shares = auth_share(&r, self.n, &self.alpha, rng);
        r_shares
            .into_iter()
            .enumerate()
            .map(|(i, share)| AuthRand {
                auth_share: share,
                clear_rand: if i == id { Some(r) } else { None },
            })
            .collect()
    }

    fn gen_triple(&self, rng: &mut impl Rng) -> (Vec<AuthShare>, Vec<AuthShare>, Vec<AuthShare>) {
        auth_triple(self.n, &self.alpha, rng)
    }
}
