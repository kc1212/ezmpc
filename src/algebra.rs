use alga::general::{AbstractMagma, Additive, Identity, Multiplicative, TwoSidedInverse};
use alga_derive::Alga;
use approx::{AbsDiffEq, RelativeEq};
use num_integer::{ExtendedGcd, Integer};
use num_traits::{One, Zero};
use quickcheck::{Arbitrary, Gen};
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

const P: u128 = 7;

#[derive(Alga, Copy, Clone, PartialEq, Eq, Debug)]
#[alga_traits(Field(Additive, Multiplicative))]
#[alga_quickcheck]
struct W(u128); // we can only hold 64-bit values

impl AbsDiffEq for W {
    type Epsilon = W;
    fn default_epsilon() -> W {
        W(0)
    }
    fn abs_diff_eq(&self, other: &W, _epsilon: W) -> bool {
        self == other
    }
}

impl RelativeEq for W {
    fn default_max_relative() -> W {
        W(0)
    }

    fn relative_eq(&self, other: &Self, _epsilon: W, _max_relative: W) -> bool {
        self == other
    }
}

impl Arbitrary for W {
    fn arbitrary<G: Gen>(g: &mut G) -> Self {
        W(u128::arbitrary(g) % P)
    }
    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.0.shrink().map(W))
    }
}

impl AbstractMagma<Additive> for W {
    fn operate(&self, right: &Self) -> Self {
        W((self.0 + right.0) % P)
    }
}
impl AbstractMagma<Multiplicative> for W {
    fn operate(&self, right: &Self) -> Self {
        W((self.0 * right.0) % P)
    }
}

impl TwoSidedInverse<Additive> for W {
    fn two_sided_inverse(&self) -> Self {
        W(P - self.0)
    }
}

impl TwoSidedInverse<Multiplicative> for W {
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
        W(inv)
    }
}

impl Identity<Additive> for W {
    fn identity() -> Self {
        W(0)
    }
}

impl Identity<Multiplicative> for W {
    fn identity() -> Self {
        W(1)
    }
}

impl Add<W> for W {
    type Output = W;

    fn add(self, rhs: W) -> W {
        AbstractMagma::<Additive>::operate(&self, &rhs)
    }
}

impl Sub<W> for W {
    type Output = W;

    fn sub(self, rhs: W) -> W {
        AbstractMagma::<Additive>::operate(
            &self,
            &TwoSidedInverse::<Additive>::two_sided_inverse(&rhs),
        )
    }
}

impl AddAssign<W> for W {
    fn add_assign(&mut self, rhs: W) {
        self.0 = self.add(rhs).0;
    }
}

impl SubAssign<W> for W {
    fn sub_assign(&mut self, rhs: W) {
        self.0 = self.sub(rhs).0;
    }
}

impl Neg for W {
    type Output = W;

    fn neg(self) -> W {
        TwoSidedInverse::<Additive>::two_sided_inverse(&self)
    }
}

impl Zero for W {
    fn zero() -> W {
        Identity::<Additive>::identity()
    }

    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl One for W {
    fn one() -> W {
        Identity::<Multiplicative>::identity()
    }
}

impl Mul<W> for W {
    type Output = W;

    fn mul(self, rhs: W) -> W {
        AbstractMagma::<Multiplicative>::operate(&self, &rhs)
    }
}

impl Div<W> for W {
    type Output = W;

    fn div(self, rhs: W) -> W {
        self.mul(TwoSidedInverse::<Multiplicative>::two_sided_inverse(&rhs))
    }
}

impl MulAssign<W> for W {
    fn mul_assign(&mut self, rhs: W) {
        self.0 = self.mul(rhs).0
    }
}

impl DivAssign<W> for W {
    fn div_assign(&mut self, rhs: W) {
        self.0 = self.div(rhs).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alga::general::Field;

    #[test]
    fn test_trait_impl() {
        fn is_field<T: Field>() {}
        is_field::<W>();
    }
}
