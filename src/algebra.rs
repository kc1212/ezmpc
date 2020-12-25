use alga::general::{AbstractMagma, Additive, Identity, Multiplicative, TwoSidedInverse};
use alga_derive::Alga;
use approx::{AbsDiffEq, RelativeEq};
use num_traits::{One, Zero};
use quickcheck::{Arbitrary, Gen};
use rand::{Rand, Rng};
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

type FpRepr = u128;
const P: FpRepr = 18446744073709551557;

#[derive(Alga, Copy, Clone, PartialEq, Eq, Debug)]
#[alga_traits(Field(Additive, Multiplicative))]
#[alga_quickcheck]
pub struct Fp(FpRepr); // we can only hold 64-bit values

impl AbsDiffEq for Fp {
    type Epsilon = Fp;
    fn default_epsilon() -> Fp {
        Fp(0)
    }
    fn abs_diff_eq(&self, other: &Fp, _epsilon: Fp) -> bool {
        self == other
    }
}

impl RelativeEq for Fp {
    fn default_max_relative() -> Fp {
        Fp(0)
    }

    fn relative_eq(&self, other: &Self, _epsilon: Fp, _max_relative: Fp) -> bool {
        self == other
    }
}

impl Arbitrary for Fp {
    fn arbitrary<G: Gen>(g: &mut G) -> Self {
        Fp(FpRepr::arbitrary(g) % P)
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
fn egcd(a: Fp, b: Fp) -> (Fp, Fp, Fp) {
    let mut s: (FpRepr, FpRepr) = (0, 1);
    let mut t: (FpRepr, FpRepr) = (1, 0);
    let mut r = (b.0, a.0);

    while !r.0.is_zero() {
        let q = r.1.clone() / r.0.clone();
        let f = |mut r: (FpRepr, FpRepr)| {
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
        let (gcd, x, _) = egcd(*self, Fp(P));
        if gcd == One::one() {
            x
        } else {
            panic!("multiplicative inverse does not exist")
        }
    }
}

impl Identity<Additive> for Fp {
    fn identity() -> Self {
        Fp(0)
    }
}

impl Identity<Multiplicative> for Fp {
    fn identity() -> Self {
        Fp(1)
    }
}

impl Add<Fp> for Fp {
    type Output = Fp;

    fn add(self, rhs: Fp) -> Fp {
        AbstractMagma::<Additive>::operate(&self, &rhs)
    }
}

impl Sub<Fp> for Fp {
    type Output = Fp;

    fn sub(self, rhs: Fp) -> Fp {
        AbstractMagma::<Additive>::operate(
            &self,
            &TwoSidedInverse::<Additive>::two_sided_inverse(&rhs),
        )
    }
}

impl AddAssign<Fp> for Fp {
    fn add_assign(&mut self, rhs: Fp) {
        self.0 = self.add(rhs).0;
    }
}

impl SubAssign<Fp> for Fp {
    fn sub_assign(&mut self, rhs: Fp) {
        self.0 = self.sub(rhs).0;
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

impl Mul<Fp> for Fp {
    type Output = Fp;

    fn mul(self, rhs: Fp) -> Fp {
        AbstractMagma::<Multiplicative>::operate(&self, &rhs)
    }
}

impl Div<Fp> for Fp {
    type Output = Fp;

    fn div(self, rhs: Fp) -> Fp {
        self.mul(TwoSidedInverse::<Multiplicative>::two_sided_inverse(&rhs))
    }
}

impl MulAssign<Fp> for Fp {
    fn mul_assign(&mut self, rhs: Fp) {
        self.0 = self.mul(rhs).0
    }
}

impl DivAssign<Fp> for Fp {
    fn div_assign(&mut self, rhs: Fp) {
        self.0 = self.div(rhs).0
    }
}

fn rand_u128<R: Rng>(rng: &mut R) -> u128 {
    let x0: u128 = rng.gen::<u64>() as u128;
    let x1: u128 = rng.gen::<u64>() as u128;
    let x = x0 + (x1 << std::mem::size_of::<u128>() * 8 / 2);
    x
}

impl Rand for Fp {
    fn rand<R: Rng>(rng: &mut R) -> Self {
        let x = rand_u128(rng);
        Fp(x % P)
    }
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
}
