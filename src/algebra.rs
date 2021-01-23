use alga::general::{AbstractMagma, Additive, Identity, Multiplicative, TwoSidedInverse};
use alga_derive::Alga;
use approx::{AbsDiffEq, RelativeEq};
use auto_ops::*;
use num_traits::{One, Pow, Zero};
use quickcheck::{Arbitrary, Gen};
use rand::{Rand, Rng};
use std::ops::{AddAssign, DivAssign, MulAssign, Neg, SubAssign};
use uint::construct_uint;

construct_uint! {
    pub struct U256(4);
}

//   340282366920938463463374607431768211297
// = 18446744073709551457 + 18446744073709551615 << 64
const P: U256 = U256([18446744073709551457, 18446744073709551615, 0, 0]);

#[derive(Alga, Copy, Clone, PartialEq, Eq, Debug)]
#[alga_traits(Field(Additive, Multiplicative))]
#[alga_quickcheck]
pub struct Fp(U256); // we can only hold 64-bit values

impl From<u128> for Fp {
    fn from(x: u128) -> Self {
        Fp(U256::from(x))
    }
}

impl From<usize> for Fp {
    fn from(x: usize) -> Self {
        Fp(U256::from(x))
    }
}

impl AbsDiffEq for Fp {
    type Epsilon = Fp;
    fn default_epsilon() -> Fp {
        Fp(U256::zero())
    }
    fn abs_diff_eq(&self, other: &Fp, _epsilon: Fp) -> bool {
        self == other
    }
}

impl RelativeEq for Fp {
    fn default_max_relative() -> Fp {
        Fp(U256::zero())
    }

    fn relative_eq(&self, other: &Self, _epsilon: Fp, _max_relative: Fp) -> bool {
        self == other
    }
}

impl Arbitrary for Fp {
    fn arbitrary<G: Gen>(g: &mut G) -> Self {
        Fp(U256::arbitrary(g) % P)
    }
    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.0.shrink().map(Fp))
    }
}

impl AbstractMagma<Additive> for Fp {
    fn operate(&self, right: &Self) -> Self {
        Fp((self.0 + right.0) % P)
    }
}
impl AbstractMagma<Multiplicative> for Fp {
    fn operate(&self, right: &Self) -> Self {
        Fp((self.0 * right.0) % P)
    }
}

impl TwoSidedInverse<Additive> for Fp {
    fn two_sided_inverse(&self) -> Self {
        Fp(P - self.0)
    }
}

/// taken from https://github.com/rust-num/num-integer/blob/19ab37c59d038e05f34d7817dd3ddd2c490d982c/src/lib.rs#L165
fn xgcd(a: Fp, b: Fp) -> (Fp, Fp, Fp) {
    let mut s: (U256, U256) = (U256::zero(), U256::one());
    let mut t: (U256, U256) = (U256::one(), U256::zero());
    let mut r = (b.0, a.0);

    while !r.0.is_zero() {
        let q = r.1.clone() / r.0.clone();
        let f = |mut r: (U256, U256)| {
            std::mem::swap(&mut r.0, &mut r.1);
            // r.0 = r.0 - q * r.1;
            let neg_qr1 = P - ((q * r.1) % P);
            r.0 = (r.0 + neg_qr1) % P;
            r
        };
        r = f(r);
        s = f(s);
        t = f(t);
    }
    (Fp(r.1), Fp(s.1), Fp(t.1))
}

impl TwoSidedInverse<Multiplicative> for Fp {
    fn two_sided_inverse(&self) -> Self {
        let (gcd, x, _) = xgcd(*self, Fp(P));
        if gcd == One::one() {
            x
        } else {
            panic!("multiplicative inverse does not exist")
        }
    }
}

impl Identity<Additive> for Fp {
    fn identity() -> Self {
        Fp(U256::zero())
    }
}

impl Identity<Multiplicative> for Fp {
    fn identity() -> Self {
        Fp(U256::one())
    }
}

impl_op_ex!(+|a: &Fp, b:  &Fp| -> Fp {
    AbstractMagma::<Additive>::operate(a, b)
});

impl_op_ex!(-|a: &Fp, b: &Fp| -> Fp { AbstractMagma::<Additive>::operate(a, &TwoSidedInverse::<Additive>::two_sided_inverse(&b)) });

impl AddAssign<Fp> for Fp {
    fn add_assign(&mut self, rhs: Fp) {
        self.0 = (*self + rhs).0;
    }
}

impl SubAssign<Fp> for Fp {
    fn sub_assign(&mut self, rhs: Fp) {
        self.0 = (*self - rhs).0
    }
}

impl Neg for Fp {
    type Output = Fp;

    fn neg(self) -> Fp {
        TwoSidedInverse::<Additive>::two_sided_inverse(&self)
    }
}

impl Zero for Fp {
    fn zero() -> Fp {
        Identity::<Additive>::identity()
    }

    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl One for Fp {
    fn one() -> Fp {
        Identity::<Multiplicative>::identity()
    }
}

impl_op_ex!(*|a: &Fp, b: &Fp| -> Fp { AbstractMagma::<Multiplicative>::operate(a, b) });

impl_op_ex!(/|a: &Fp, b: &Fp| -> Fp {
    a * TwoSidedInverse::<Multiplicative>::two_sided_inverse(b)
});

impl MulAssign<Fp> for Fp {
    fn mul_assign(&mut self, rhs: Fp) {
        self.0 = (*self * rhs).0
    }
}

impl DivAssign<Fp> for Fp {
    fn div_assign(&mut self, rhs: Fp) {
        self.0 = (*self / rhs).0
    }
}

impl Pow<Fp> for Fp {
    type Output = Fp;

    fn pow(self, rhs: Fp) -> Self::Output {
        if P == U256::one() {
            return Fp(U256::zero());
        }

        let mut base = self.0;
        let mut result = U256::one();
        let mut exp = rhs.0;

        base = base % P;
        while exp > U256::zero() {
            if exp % U256::from(2) == U256::one() {
                result = result * base % P;
            }
            exp = exp >> U256::one();
            base = base * base % P
        }
        Fp(result)
    }
}

impl Rand for Fp {
    fn rand<R: Rng>(rng: &mut R) -> Self {
        let mut buf: [u64; 4] = [0; 4];
        for i in 0..4 {
            buf[i] = rng.gen::<u64>();
        }
        Fp(U256(buf) % P)
    }
}

impl std::iter::Sum for Fp {
    fn sum<I: Iterator<Item = Fp>>(iter: I) -> Self {
        let mut out = Zero::zero();
        for x in iter {
            out += x;
        }
        out
    }
}

pub fn to_le_bytes(x: &Fp) -> [u8; 32] {
    let mut out: [u8; 32] = [0; 32];
    x.0.to_little_endian(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alga::general::Field;

    #[test]
    fn test_trait_impl() {
        fn is_field<T: Field>() {}
        is_field::<Fp>();
    }

    #[quickcheck]
    fn prop_diffie_hellman(g: Fp, a: Fp, b: Fp) -> bool {
        let ga = g.pow(a);
        let gb = g.pow(b);
        gb.pow(a) == ga.pow(b)
    }
}
