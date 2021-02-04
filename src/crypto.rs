//! This module contains our cryptographic primitives.

use crate::algebra::Fp;

use auto_ops::*;
use num_traits::Zero;
use rand::Rng;
use serde::{Deserialize, Serialize};

/// This structure represents an authenticated share.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuthShare {
    pub share: Fp,
    pub mac: Fp,
}

impl AuthShare {
    /// This function multiplies an authenticated share by a clear value.
    pub fn mul_clear(&self, rhs: &Fp) -> Self {
        Self {
            share: &self.share * rhs,
            mac: &self.mac * rhs,
        }
    }

    /// This function adds an authenticated share by a clear value.
    /// Due to the way sharing works, only one party should have `update_share` to `true`,
    /// otherwise the new share will be wrong.
    pub fn add_clear(&self, rhs: &Fp, alpha_share: &Fp, update_share: bool) -> AuthShare {
        let mut out = self.clone();
        if update_share {
            out.share += rhs;
        }
        out.mac += alpha_share * rhs;
        out
    }
}

impl_op_ex!(+|a: &AuthShare, b: &AuthShare| -> AuthShare {
    AuthShare {
        share: &a.share + &b.share,
        mac: &a.mac + &b.mac,
    }
});

impl_op_ex!(-|a: &AuthShare, b: &AuthShare| -> AuthShare {
    AuthShare {
        share: &a.share - &b.share,
        mac: &a.mac - &b.mac,
    }
});

/// Share a field element `secret` into `n` shares.
/// Use the authenticated version of this function `auth_share` unless there is a very specific reason.
pub fn unauth_share(secret: &Fp, n: usize, rng: &mut impl Rng) -> Vec<Fp> {
    let mut out: Vec<Fp> = vec![Fp::zero(); n];
    let mut sum = Fp::zero();
    for i in 0..(n - 1) {
        let r = Fp::random(rng);
        sum += &r;
        out[i] = r;
    }

    let final_share = secret - &sum;
    out[n - 1] = final_share;
    out
}

/// Reconstruct shares of a field element into the original one.
/// Use the authenticated version of this function `auth_combine` unless there is a very specific reason.
pub fn unauth_combine(shares: &Vec<Fp>) -> Fp {
    let mut out = Fp::zero();
    for share in shares {
        out += share;
    }
    out
}

/// Generate a sharing of a random triple for `n` parties.
/// Use the authenticated version of this function `auth_triple` unless there is a very specific reason.
pub fn unauth_triple(n: usize, rng: &mut impl Rng) -> (Vec<Fp>, Vec<Fp>, Vec<Fp>) {
    let a: Fp = Fp::random(rng);
    let b: Fp = Fp::random(rng);
    let c: Fp = &a * &b;
    (unauth_share(&a, n, rng), unauth_share(&b, n, rng), unauth_share(&c, n, rng))
}

/// Share a field element `secret` into `n` shares,
/// where `alpha` is the global MAC key.
pub fn auth_share(secret: &Fp, n: usize, alpha: &Fp, rng: &mut impl Rng) -> Vec<AuthShare> {
    let mac_on_secret = secret * alpha;
    let reg_shares = unauth_share(secret, n, rng);
    let mac_shares = unauth_share(&mac_on_secret, n, rng);

    reg_shares
        .into_iter()
        .zip(mac_shares)
        .map(|(share, mac)| AuthShare { share, mac })
        .collect()
}

/// Generate a sharing of a random triple for `n` parties where `alpha` is the global MAC key.
pub fn auth_triple(n: usize, alpha: &Fp, rng: &mut impl Rng) -> (Vec<AuthShare>, Vec<AuthShare>, Vec<AuthShare>) {
    let a: Fp = Fp::random(rng);
    let b: Fp = Fp::random(rng);
    let c: Fp = &a * &b;
    (
        auth_share(&a, n, alpha, rng),
        auth_share(&b, n, alpha, rng),
        auth_share(&c, n, alpha, rng),
    )
}

pub mod commit {
    use crate::algebra::Fp;

    use bincode;
    use rand::Rng;
    use serde::{Deserialize, Serialize};
    use sha3;
    use sha3::Digest;
    use std::fmt;

    /// This is the structure that represents a commitment.
    #[derive(Serialize, Deserialize, Clone)]
    pub struct Commitment {
        c: [u8; 32],
    }

    impl fmt::Debug for Commitment {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Commitment")
                .field("c", &format_args!("{:x?}...", self.c.get(0..2).unwrap()))
                .finish()
        }
    }

    /// This is the structure that represents an opening of a commitment.
    #[derive(Serialize, Deserialize, Clone)]
    pub struct Opening {
        v: Fp,
        r: [u8; 32],
    }

    impl fmt::Debug for Opening {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Opening")
                .field("v", &self.v)
                .field("r", &format_args!("{:x?}...", self.r.get(0..2).unwrap()))
                .finish()
        }
    }

    impl Opening {
        /// Returns the committed value.
        pub fn get_v(&self) -> Fp {
            self.v.clone()
        }
    }

    #[derive(Clone, Debug)]
    pub struct Scheme {
        // See Fig. 1 of https://eprint.iacr.org/2012/642.pdf
    }

    impl Scheme {
        /// Generates a commitment for `secret` and an opening.
        pub fn commit(&self, secret: Fp, rng: &mut impl Rng) -> (Commitment, Opening) {
            let r: [u8; 32] = rng.gen();
            let v = bincode::serialize(&secret).expect("serialization failed");
            let mut hasher = sha3::Sha3_256::new();
            hasher.update(&r);
            hasher.update(&v);
            let c = hasher.finalize().into();
            (Commitment { c }, Opening { v: secret, r })
        }

        /// Use the commitment `com` and the opening `opening` to verify whether the committer honestly opened its value.
        pub fn verify(&self, opening: &Opening, com: &Commitment) -> bool {
            let mut hasher = sha3::Sha3_256::new();
            let v = bincode::serialize(&opening.v).expect("serialization failed");
            hasher.update(&opening.r);
            hasher.update(&v);
            let c_prime: [u8; 32] = hasher.finalize().into();
            com.c == c_prime
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use itertools::izip;
    use num_traits::{One, Zero};
    use quickcheck_macros::quickcheck;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha20Rng;

    const TEST_SEED: [u8; 32] = [8u8; 32];

    #[test]
    fn test_fp_rand() {
        let rng = &mut ChaCha20Rng::from_seed(TEST_SEED);
        let a: Fp = Fp::random(rng);
        let b: Fp = Fp::random(rng);
        assert_ne!(a, b);
    }

    #[test]
    fn test_unauth_sharing() {
        let n = 4;
        let rng = &mut ChaCha20Rng::from_seed(TEST_SEED);
        let secret: Fp = Fp::random(rng);
        let shares = unauth_share(&secret, n, rng);
        let recovered = unauth_combine(&shares);
        assert_eq!(secret, recovered);

        // test linearity
        let secret2: Fp = Fp::random(rng);
        let shares2 = unauth_share(&secret2, n, rng);

        let new_shares: Vec<Fp> = shares.iter().zip(&shares2).map(|(x, y)| x + y).collect();
        assert_eq!(&secret + &secret2, unauth_combine(&new_shares));

        let const_term: Fp = Fp::random(rng);
        assert_eq!(&secret * &const_term, unauth_combine(&shares.iter().map(|s| s * &const_term).collect()));
        assert_eq!(&secret2 * &const_term, unauth_combine(&shares2.iter().map(|s| s * &const_term).collect()));
    }

    fn unauth_triple_protocol(x: Fp, y: Fp, n: usize, rng: &mut impl Rng) {
        let (a_boxes, b_boxes, c_boxes) = unauth_triple(n, rng);
        assert_eq!(unauth_combine(&c_boxes), unauth_combine(&a_boxes) * unauth_combine(&b_boxes));

        let x_boxes = unauth_share(&x, n, rng);
        let y_boxes = unauth_share(&y, n, rng);
        assert_eq!(unauth_combine(&x_boxes), x);
        assert_eq!(unauth_combine(&y_boxes), y);

        let e_boxes: Vec<Fp> = x_boxes.into_iter().zip(&a_boxes).map(|(x, a)| x - a).collect();
        let d_boxes: Vec<Fp> = y_boxes.into_iter().zip(&b_boxes).map(|(y, b)| y - b).collect();

        let e = unauth_combine(&e_boxes);
        let d = unauth_combine(&d_boxes);
        assert_eq!(e, &x - &unauth_combine(&a_boxes));
        assert_eq!(d, &y - &unauth_combine(&b_boxes));

        let ed = &e * &d;

        let z_boxes = izip!(&c_boxes, &b_boxes, &a_boxes)
            .map(|(c_box, b_box, a_box)| {
                let mut v = c_box.clone();
                v += &e * b_box;
                v += &d * a_box;
                v
            })
            .collect();

        let z = unauth_combine(&z_boxes) + ed;
        assert_eq!(x * y, z);
    }

    #[test]
    fn test_unauth_triple() {
        let n = 4;
        let rng = &mut ChaCha20Rng::from_seed(TEST_SEED);
        {
            let x: Fp = Fp::one();
            let y: Fp = Fp::one() + Fp::one();
            unauth_triple_protocol(x, y, n, rng);
        }
        {
            let x: Fp = Fp::random(rng);
            let y: Fp = Fp::random(rng);
            unauth_triple_protocol(x, y, n, rng);
        }
    }

    fn auth_combine_no_assert(shares: &Vec<AuthShare>, alpha_shares: &Vec<Fp>) -> (bool, Fp) {
        let x = unauth_combine(&shares.iter().map(|x| x.share.clone()).collect());

        // in practice these ds values are committed first before revealing
        let ds: Vec<_> = alpha_shares.into_iter().zip(shares).map(|(a, share)| a * &x - &share.mac).collect();
        let d = unauth_combine(&ds);
        (Fp::zero() == d, x)
    }

    fn auth_combine(shares: &Vec<AuthShare>, alpha_shares: &Vec<Fp>) -> Fp {
        let (ok, out) = auth_combine_no_assert(shares, alpha_shares);
        assert!(ok);
        out
    }

    #[test]
    fn test_auth_arithmetic() {
        let rng = &mut ChaCha20Rng::from_seed(TEST_SEED);
        let n = 4;
        let alpha: Fp = Fp::random(rng);
        let alpha_shares = unauth_share(&alpha, n, rng);
        let a: Fp = Fp::random(rng);
        let b: Fp = Fp::random(rng);
        let const_c: Fp = Fp::random(rng);

        let a_shares = auth_share(&a, n, &alpha, rng);
        let b_shares = auth_share(&b, n, &alpha, rng);

        // check a+b
        let a_add_b_shares: Vec<_> = a_shares.iter().zip(&b_shares).map(|(a, b)| a + b).collect();
        assert_eq!(&a + &b, auth_combine(&a_add_b_shares, &alpha_shares));

        // check a-b
        let a_sub_b_shares: Vec<_> = a_shares.iter().zip(&b_shares).map(|(a, b)| a - b).collect();
        assert_eq!(&a - &b, auth_combine(&a_sub_b_shares, &alpha_shares));

        // check mul by constant
        let mul_const_shares: Vec<_> = a_shares.iter().map(|share| share.mul_clear(&const_c)).collect();
        assert_eq!(&a * &const_c, auth_combine(&mul_const_shares, &alpha_shares));

        // check add by constant
        let add_const_shares: Vec<_> = b_shares
            .iter()
            .enumerate()
            .map(|(i, share)| share.add_clear(&const_c, &alpha_shares[i], i == 0))
            .collect();
        assert_eq!(b + const_c, auth_combine(&add_const_shares, &alpha_shares));
    }

    #[test]
    fn test_auth_share() {
        let rng = &mut ChaCha20Rng::from_seed(TEST_SEED);
        let n = 4;
        let secret: Fp = Fp::random(rng);
        let alpha: Fp = Fp::random(rng);
        let shares = auth_share(&secret, n, &alpha, rng);

        let result = auth_combine(&shares, &unauth_share(&alpha, n, rng));
        assert_eq!(secret, result);

        // test failure: bad MAC
        let mut bad_shares = shares.clone();
        bad_shares[0].mac += &Fp::one();
        let bad_result = auth_combine_no_assert(&bad_shares, &unauth_share(&alpha, n, rng));
        assert_eq!((false, secret), bad_result);
    }

    fn auth_triple_protocol(x: Fp, y: Fp, n: usize, alpha: &Fp, rng: &mut impl Rng) {
        let alpha_shares = unauth_share(alpha, n, rng);
        let (a_boxes, b_boxes, c_boxes) = auth_triple(n, alpha, rng);
        assert_eq!(
            auth_combine(&c_boxes, &alpha_shares),
            auth_combine(&a_boxes, &alpha_shares) * auth_combine(&b_boxes, &alpha_shares)
        );

        let x_boxes = auth_share(&x, n, alpha, rng);
        let y_boxes = auth_share(&y, n, alpha, rng);
        assert_eq!(auth_combine(&x_boxes, &alpha_shares), x);
        assert_eq!(auth_combine(&y_boxes, &alpha_shares), y);

        let e_boxes: Vec<_> = x_boxes.iter().zip(&a_boxes).map(|(x, a)| x - a).collect();
        let d_boxes: Vec<_> = y_boxes.iter().zip(&b_boxes).map(|(y, b)| y - b).collect();

        let e = auth_combine(&e_boxes, &alpha_shares);
        let d = auth_combine(&d_boxes, &alpha_shares);
        assert_eq!(e, &x - &auth_combine(&a_boxes, &alpha_shares));
        assert_eq!(d, &y - &auth_combine(&b_boxes, &alpha_shares));

        let ed = &e * &d;

        let z_boxes: Vec<_> = izip!(0..n, &alpha_shares, &c_boxes, &b_boxes, &a_boxes)
            .map(|(i, alpha_share, c_box, b_box, a_box)| {
                let eb_box = b_box.mul_clear(&e);
                let da_box = a_box.mul_clear(&d);
                (c_box + eb_box + da_box).add_clear(&ed, alpha_share, i == 0)
            })
            .collect();

        let z = auth_combine(&z_boxes, &alpha_shares);
        assert_eq!(x * y, z);
    }

    #[test]
    fn test_auth_triple() {
        let n = 4;
        let rng = &mut ChaCha20Rng::from_seed(TEST_SEED);
        {
            let x = Fp::one();
            let y = Fp::one() + Fp::one();
            let alpha = Fp::random(rng);
            auth_triple_protocol(x, y, n, &alpha, rng);
        }
        {
            let x = Fp::random(rng);
            let y = Fp::random(rng);
            let alpha: Fp = Fp::random(rng);
            auth_triple_protocol(x, y, n, &alpha, rng);
        }
    }

    #[quickcheck]
    fn prop_commitment(secret: Fp) -> bool {
        let rng = &mut ChaCha20Rng::from_seed(TEST_SEED);
        let scheme = commit::Scheme {};
        let (commitment, opening) = scheme.commit(secret, rng);
        scheme.verify(&opening, &commitment)
    }

    #[quickcheck]
    fn prop_bad_commitment(secret: Fp) -> bool {
        let rng = &mut ChaCha20Rng::from_seed(TEST_SEED);
        let scheme = commit::Scheme {};
        let (commitment, _) = scheme.commit(secret.clone(), rng);

        let secret_bad = secret + Fp::one();
        let (_, bad_opening) = scheme.commit(secret_bad, rng);
        !scheme.verify(&bad_opening, &commitment)
    }
}
