use ff::*;
use rand::Rng;
use std::ops::{Add, Mul, Sub};

#[derive(PrimeField)]
#[PrimeFieldModulus = "52435875175126190479447740508185965837690552500527637822603658699938581184513"]
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

pub fn share(secret: &Fp, n: usize, rng: &mut impl Rng) -> Vec<Fp> {
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

pub fn combine(shares: &Vec<Fp>) -> Fp {
    let mut out = Fp::zero();
    for share in shares {
        out.add_assign(share);
    }
    out
}

pub fn generate_triple(n: usize, rng: &mut impl Rng) -> (Vec<Fp>, Vec<Fp>, Vec<Fp>) {
    let a: Fp = rng.gen();
    let b: Fp = rng.gen();
    let c: Fp = a * b;
    (share(&a, n, rng), share(&b, n, rng), share(&c, n, rng))
}

pub fn random_sharing(n: usize, rng: &mut impl Rng) -> (Fp, Vec<Fp>) {
    let r = rng.gen();
    (r, share(&r, n, rng))
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
    fn test_sharing() {
        let n = 4;
        let rng = &mut XorShiftRng::from_seed(SEED);
        let secret: Fp = rng.gen();
        let shares = share(&secret, n, rng);
        let recovered = combine(&shares);
        assert_eq!(secret, recovered);

        // test linearity
        let secret2: Fp = rng.gen();
        let shares2 = share(&secret2, n, rng);

        let new_shares: Vec<Fp> = shares.iter().zip(&shares2).map(|(x, y)| x + y).collect();
        assert_eq!(secret + secret2, combine(&new_shares));

        let const_term: Fp = rng.gen();
        assert_eq!(
            secret * const_term,
            combine(&shares.iter().map(|s| s * &const_term).collect())
        );
        assert_eq!(
            secret2 * const_term,
            combine(&shares2.iter().map(|s| s * &const_term).collect())
        );
    }

    fn triple_protocol(x: Fp, y: Fp, n: usize, rng: &mut impl Rng) {
        let (a_boxes, b_boxes, c_boxes) = generate_triple(n, rng);
        assert_eq!(combine(&c_boxes), combine(&a_boxes) * combine(&b_boxes));

        let x_boxes = share(&x, n, rng);
        let y_boxes = share(&y, n, rng);
        assert_eq!(combine(&x_boxes), x);
        assert_eq!(combine(&y_boxes), y);

        let e_boxes: Vec<Fp> = x_boxes.iter().zip(&a_boxes).map(|(x, a)| x - a).collect();
        let d_boxes: Vec<Fp> = y_boxes.iter().zip(&b_boxes).map(|(y, b)| y - b).collect();

        let e = combine(&e_boxes);
        let d = combine(&d_boxes);
        assert_eq!(e, x - combine(&a_boxes));
        assert_eq!(d, y - combine(&b_boxes));

        let ed = &e * &d;

        let z_boxes = izip!(&c_boxes, &b_boxes, &a_boxes)
            .map(|(c_box, b_box, a_box)| {
                let mut v = c_box.clone();
                v.add_assign(&(&e * b_box));
                v.add_assign(&(&d * a_box));
                v
            })
            .collect();

        let z = combine(&z_boxes) + ed;
        assert_eq!(x * y, z);
    }

    #[test]
    fn test_triple() {
        let rng = &mut XorShiftRng::from_seed(SEED);
        let x: Fp = Fp::one();
        let y: Fp = Fp::one() + Fp::one();
        triple_protocol(x, y, 4, rng);
    }
}
