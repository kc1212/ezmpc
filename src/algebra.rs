use alga::general::{AbstractMagma, Additive, Identity, Multiplicative, TwoSidedInverse};
use alga_derive::Alga;
use approx::{AbsDiffEq, RelativeEq};
use num_traits::{One, Zero};
use quickcheck::{Arbitrary, Gen};
use rand::{Rand, Rng};
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

const P: u128 = 18446744073709551557;

#[derive(Alga, Copy, Clone, PartialEq, Eq, Debug)]
#[alga_traits(Field(Additive, Multiplicative))]
#[alga_quickcheck]
pub struct Fp(u128); // we can only hold 64-bit values

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
        Fp(u128::arbitrary(g) % P)
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

impl TwoSidedInverse<Multiplicative> for Fp {
    fn two_sided_inverse(&self) -> Self {
        let (mut a, mut m, mut x0, mut inv) = (self.0, P, 0u128, 1u128);
        while a > 1 {
            let tmp = ((a / m) * x0) % P;
            if tmp > inv {
                inv = ((P + inv) - tmp) % P;
            } else {
                inv -= tmp;
            }
            a = a % m;
            std::mem::swap(&mut a, &mut m);
            std::mem::swap(&mut x0, &mut inv);
        }
        Fp(inv)
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

impl Rand for Fp {
    fn rand<R: Rng>(rng: &mut R) -> Self {
        let x0: u128 = rng.gen::<u64>() as u128;
        let x1: u128 = rng.gen::<u64>() as u128;
        let x = x0 + (x1 << std::mem::size_of::<u128>() * 8 / 2);
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
