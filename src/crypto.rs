use ff::*;
use rand::Rng;
use std::ops::{Add, Mul, Sub};

// sage: p = previous_prime(2^80)
// sage: GF(p).primitive_element()
#[derive(PrimeField)]
#[PrimeFieldModulus = "1208925819614629174706111"]
#[PrimeFieldGenerator = "7"]
pub struct Fp(FpRepr);

impl Add for Fp {
    type Output = Fp;
    fn add(self, rhs: Self) -> Self::Output {
        let mut v = self;
        v.add_assign(&rhs);
        v
    }
}

impl Add for &Fp {
    type Output = Fp;
    fn add(self, rhs: Self) -> Self::Output {
        let mut v = self.clone();
        v.add_assign(&rhs);
        v
    }
}

impl Sub for Fp {
    type Output = Fp;
    fn sub(self, rhs: Self) -> Self::Output {
        let mut v = self;
        v.sub_assign(&rhs);
        v
    }
}

impl Sub for &Fp {
    type Output = Fp;
    fn sub(self, rhs: Self) -> Self::Output {
        let mut v = self.clone();
        v.sub_assign(&rhs);
        v
    }
}

impl Mul for Fp {
    type Output = Fp;
    fn mul(self, rhs: Self) -> Self::Output {
        let mut v = self;
        v.mul_assign(&rhs);
        v
    }
}

impl Mul for &Fp {
    type Output = Fp;

    fn mul(self, rhs: Self) -> Self::Output {
        let mut v = self.clone();
        v.mul_assign(&rhs);
        v
    }
}

#[derive(Copy, Clone, Debug)]
pub struct AuthShare {
    pub share: Fp,
    pub mac: Fp,
}

impl AuthShare {
    fn mul_const_assign(&mut self, rhs: &Fp) {
        self.share.mul_assign(rhs);
        self.mac.mul_assign(rhs);
    }

    fn mul_const(&self, rhs: &Fp) -> Self {
        let mut out = self.clone();
        out.mul_const_assign(rhs);
        out
    }

    fn add_const_assign(&mut self, rhs: &Fp, alpha_share: &Fp, update_share: bool) {
        if update_share {
            self.share.add_assign(rhs);
        }
        self.mac.add_assign(&(alpha_share * rhs));
    }

    fn add_const(&self, rhs: &Fp, alpha_share: &Fp, update_share: bool) -> AuthShare {
        let mut out = self.clone();
        out.add_const_assign(rhs, alpha_share, update_share);
        out
    }
}

impl Add for AuthShare {
    type Output = AuthShare;
    fn add(self, rhs: Self) -> Self::Output {
        AuthShare {
            share: self.share + rhs.share,
            mac: self.mac + rhs.mac,
        }
    }
}

impl Add for &AuthShare {
    type Output = AuthShare;
    fn add(self, rhs: Self) -> Self::Output {
        AuthShare {
            share: self.share + rhs.share,
            mac: self.mac + rhs.mac,
        }
    }
}

impl Sub for AuthShare {
    type Output = AuthShare;
    fn sub(self, rhs: Self) -> Self::Output {
        AuthShare {
            share: self.share - rhs.share,
            mac: self.mac - rhs.mac,
        }
    }
}

impl Sub for &AuthShare {
    type Output = AuthShare;
    fn sub(self, rhs: Self) -> Self::Output {
        AuthShare {
            share: self.share - rhs.share,
            mac: self.mac - rhs.mac,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct AuthRand {
    pub auth_share: AuthShare,
    pub clear_rand: Option<Fp>,
}

pub fn unauth_share(secret: &Fp, n: usize, rng: &mut impl Rng) -> Vec<Fp> {
    let mut out: Vec<Fp> = vec![Fp::zero(); n];
    let mut sum = Fp::zero();
    for i in 0..(n - 1) {
        let r: Fp = rng.gen();
        sum.add_assign(&r); // sum += r
        out[i] = r;
    }

    let mut final_share = secret.clone();
    final_share.sub_assign(&sum);
    out[n - 1] = final_share;
    out
}

pub fn unauth_combine(shares: &Vec<Fp>) -> Fp {
    let mut out = Fp::zero();
    for share in shares {
        out.add_assign(share);
    }
    out
}

pub fn unauth_triple(n: usize, rng: &mut impl Rng) -> (Vec<Fp>, Vec<Fp>, Vec<Fp>) {
    let a: Fp = rng.gen();
    let b: Fp = rng.gen();
    let c: Fp = a * b;
    (
        unauth_share(&a, n, rng),
        unauth_share(&b, n, rng),
        unauth_share(&c, n, rng),
    )
}

pub fn auth_share(secret: &Fp, n: usize, alpha: &Fp, rng: &mut impl Rng) -> Vec<AuthShare> {
    let mac_on_secret = secret * alpha;
    let reg_shares = unauth_share(&secret, n, rng);
    let mac_shares = unauth_share(&mac_on_secret, n, rng);

    reg_shares
        .into_iter()
        .zip(mac_shares)
        .map(|(share, mac)| AuthShare { share, mac })
        .collect()
}

pub fn auth_triple(
    n: usize,
    alpha: &Fp,
    rng: &mut impl Rng,
) -> (Vec<AuthShare>, Vec<AuthShare>, Vec<AuthShare>) {
    let a: Fp = rng.gen();
    let b: Fp = rng.gen();
    let c: Fp = a * b;
    (
        auth_share(&a, n, alpha, rng),
        auth_share(&b, n, alpha, rng),
        auth_share(&c, n, alpha, rng),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::izip;
    use rand::{Rng, SeedableRng, XorShiftRng};
    const SEED: [u32; 4] = [0x5dbe6259, 0x8d313d76, 0x3237db17, 0xe5bc0654];

    #[test]
    fn test_fp_rand() {
        let rng = &mut XorShiftRng::from_seed(SEED);
        let a: Fp = rng.gen();
        let b: Fp = rng.gen();
        assert_ne!(a, b);
    }

    #[test]
    fn test_unauth_sharing() {
        let n = 4;
        let rng = &mut XorShiftRng::from_seed(SEED);
        let secret: Fp = rng.gen();
        let shares = unauth_share(&secret, n, rng);
        let recovered = unauth_combine(&shares);
        assert_eq!(secret, recovered);

        // test linearity
        let secret2: Fp = rng.gen();
        let shares2 = unauth_share(&secret2, n, rng);

        let new_shares: Vec<Fp> = shares.iter().zip(&shares2).map(|(x, y)| x + y).collect();
        assert_eq!(secret + secret2, unauth_combine(&new_shares));

        let const_term: Fp = rng.gen();
        assert_eq!(
            secret * const_term,
            unauth_combine(&shares.iter().map(|s| s * &const_term).collect())
        );
        assert_eq!(
            secret2 * const_term,
            unauth_combine(&shares2.iter().map(|s| s * &const_term).collect())
        );
    }

    fn unauth_triple_protocol(x: Fp, y: Fp, n: usize, rng: &mut impl Rng) {
        let (a_boxes, b_boxes, c_boxes) = unauth_triple(n, rng);
        assert_eq!(
            unauth_combine(&c_boxes),
            unauth_combine(&a_boxes) * unauth_combine(&b_boxes)
        );

        let x_boxes = unauth_share(&x, n, rng);
        let y_boxes = unauth_share(&y, n, rng);
        assert_eq!(unauth_combine(&x_boxes), x);
        assert_eq!(unauth_combine(&y_boxes), y);

        let e_boxes: Vec<Fp> = x_boxes.iter().zip(&a_boxes).map(|(x, a)| x - a).collect();
        let d_boxes: Vec<Fp> = y_boxes.iter().zip(&b_boxes).map(|(y, b)| y - b).collect();

        let e = unauth_combine(&e_boxes);
        let d = unauth_combine(&d_boxes);
        assert_eq!(e, x - unauth_combine(&a_boxes));
        assert_eq!(d, y - unauth_combine(&b_boxes));

        let ed = &e * &d;

        let z_boxes = izip!(&c_boxes, &b_boxes, &a_boxes)
            .map(|(c_box, b_box, a_box)| {
                let mut v = c_box.clone();
                v.add_assign(&(&e * b_box));
                v.add_assign(&(&d * a_box));
                v
            })
            .collect();

        let z = unauth_combine(&z_boxes) + ed;
        assert_eq!(x * y, z);
    }

    #[test]
    fn test_unauth_triple() {
        let n = 4;
        let rng = &mut XorShiftRng::from_seed(SEED);
        {
            let x: Fp = Fp::one();
            let y: Fp = Fp::one() + Fp::one();
            unauth_triple_protocol(x, y, n, rng);
        }
        {
            let x: Fp = rng.gen();
            let y: Fp = rng.gen();
            unauth_triple_protocol(x, y, n, rng);
        }
    }

    fn auth_combine_no_assert(shares: &Vec<AuthShare>, alpha_shares: &Vec<Fp>) -> (bool, Fp) {
        let x = unauth_combine(&shares.iter().map(|x| x.share).collect());

        // in practice these ds values are committed first before revealing
        let ds: Vec<_> = alpha_shares
            .into_iter()
            .zip(shares)
            .map(|(a, share)| a * &x - share.mac)
            .collect();
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
        let rng = &mut XorShiftRng::from_seed(SEED);
        let n = 4;
        let alpha: Fp = rng.gen();
        let alpha_shares = unauth_share(&alpha, n, rng);
        let a: Fp = rng.gen();
        let b: Fp = rng.gen();
        let const_c: Fp = rng.gen();

        let a_shares = auth_share(&a, n, &alpha, rng);
        let b_shares = auth_share(&b, n, &alpha, rng);

        // check a+b
        let a_add_b_shares: Vec<_> = a_shares.iter().zip(&b_shares).map(|(a, b)| a + b).collect();
        assert_eq!(a + b, auth_combine(&a_add_b_shares, &alpha_shares));

        // check a-b
        let a_sub_b_shares: Vec<_> = a_shares.iter().zip(&b_shares).map(|(a, b)| a - b).collect();
        assert_eq!(a - b, auth_combine(&a_sub_b_shares, &alpha_shares));

        // check mul by constant
        let mul_const_shares: Vec<_> = a_shares
            .iter()
            .map(|share| share.mul_const(&const_c))
            .collect();
        assert_eq!(a * const_c, auth_combine(&mul_const_shares, &alpha_shares));

        // check add by constant
        let add_const_shares: Vec<_> = b_shares
            .iter()
            .enumerate()
            .map(|(i, share)| share.add_const(&const_c, &alpha_shares[i], i == 0))
            .collect();
        assert_eq!(b + const_c, auth_combine(&add_const_shares, &alpha_shares));
    }

    #[test]
    fn test_auth_share() {
        let rng = &mut XorShiftRng::from_seed(SEED);
        let n = 4;
        let secret: Fp = rng.gen();
        let alpha: Fp = rng.gen();
        let shares = auth_share(&secret, n, &alpha, rng);

        let result = auth_combine(&shares, &unauth_share(&alpha, n, rng));
        assert_eq!(secret, result);

        // test failure: bad MAC
        let mut bad_shares = shares.clone();
        bad_shares[0].mac.add_assign(&Fp::one());
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
        assert_eq!(e, x - auth_combine(&a_boxes, &alpha_shares));
        assert_eq!(d, y - auth_combine(&b_boxes, &alpha_shares));

        let ed = &e * &d;

        let z_boxes: Vec<_> = izip!(0..n, &alpha_shares, &c_boxes, &b_boxes, &a_boxes)
            .map(|(i, alpha_share, c_box, b_box, a_box)| {
                let eb_box = b_box.mul_const(&e);
                let da_box = a_box.mul_const(&d);
                (c_box + &eb_box + da_box).add_const(&ed, alpha_share, i == 0)
            })
            .collect();

        let z = auth_combine(&z_boxes, &alpha_shares);
        assert_eq!(x * y, z);
    }

    #[test]
    fn test_auth_triple() {
        let n = 4;
        let rng = &mut XorShiftRng::from_seed(SEED);
        {
            let x: Fp = Fp::one();
            let y: Fp = Fp::one() + Fp::one();
            let alpha: Fp = rng.gen();
            auth_triple_protocol(x, y, n, &alpha, rng);
        }
        {
            let x: Fp = rng.gen();
            let y: Fp = rng.gen();
            let alpha: Fp = rng.gen();
            auth_triple_protocol(x, y, n, &alpha, rng);
        }
    }
}
