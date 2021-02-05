use auto_ops::*;
use ff::*;
use num_traits::{One, Zero};
use quickcheck::{Arbitrary, Gen};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::ops::*;

#[derive(PrimeField, Serialize, Deserialize)]
#[PrimeFieldModulus = "52435875175126190479447740508185965837690552500527637822603658699938581184513"]
#[PrimeFieldGenerator = "7"]
#[PrimeFieldReprEndianness = "little"]
struct InnerFp([u64; 4]);
const LIMB_COUNT: usize = 4; // this value must match the size of InnerFp

/// Fp is a prime field element.
/// It is a wrapper type around the type generate by the `ff` crate
/// because we want to implement our own operators.
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq, Debug)]
pub struct Fp(InnerFp);

impl Fp {
    /// Generate a random field element.
    pub fn random(rng: &mut impl Rng) -> Fp {
        Fp(InnerFp::random(rng))
    }
}

impl_op_ex!(+|a: &Fp, b:  &Fp| -> Fp {
    let mut result = a.clone();
    result.add_assign(b);
    result
});

impl_op_ex!(-|a: &Fp, b: &Fp| -> Fp {
    let mut result = a.clone();
    result.sub_assign(b);
    result
});

impl_op_ex!(*|a: &Fp, b: &Fp| -> Fp {
    let mut result = a.clone();
    result.mul_assign(b);
    result
});

impl_op_ex!(/|a: &Fp, b: &Fp| -> Fp {
    let mut result = a.clone();
    result /= b;
    result
});

impl_op!(+= |a: &mut Fp, b: &Fp| {
    a.0.add_assign(b.0)
});

impl_op!(+= |a: &mut Fp, b: Fp| {
    a.0.add_assign(&(b.0))
});

impl_op!(-= |a: &mut Fp, b: &Fp| {
    a.0.sub_assign(b.0)
});

impl_op!(-= |a: &mut Fp, b: Fp| {
    a.0.sub_assign(&(b.0))
});

impl_op!(*= |a: &mut Fp, b: &Fp| {
    a.0.mul_assign(b.0)
});

impl_op!(*= |a: &mut Fp, b: Fp| {
    a.0.mul_assign(&(b.0))
});

impl_op!(/= |a: &mut Fp, b: &Fp| {
    a.0.mul_assign(b.0.invert().unwrap())
});

impl_op!(/= |a: &mut Fp, b: Fp| {
    a.0.mul_assign(b.0.invert().unwrap())
});

impl Neg for Fp {
    type Output = Fp;

    fn neg(self) -> Fp {
        Fp::zero() - self
    }
}

impl Zero for Fp {
    fn zero() -> Fp {
        Fp(InnerFp::zero())
    }

    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl One for Fp {
    fn one() -> Fp {
        Fp(InnerFp::one())
    }
}

impl std::iter::Sum for Fp {
    fn sum<I: Iterator<Item = Fp>>(iter: I) -> Self {
        let mut out = Fp::zero();
        for x in iter {
            out += &x;
        }
        out
    }
}

impl Arbitrary for Fp {
    fn arbitrary<G: Gen>(g: &mut G) -> Self {
        // TODO use u64 for now
        u64::arbitrary(g).into()
    }
}

impl From<u64> for Fp {
    fn from(x: u64) -> Self {
        Fp(x.into())
    }
}

impl From<usize> for Fp {
    fn from(x: usize) -> Self {
        Fp((x as u64).into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn prop_serialization(x: Fp) -> bool {
        // consider using serde_test crate
        let buf = bincode::serialize(&x).unwrap();
        x == bincode::deserialize(&buf).unwrap()
    }

    #[quickcheck]
    fn prop_limb_count(x: Fp) -> bool {
        x.0 .0.len() == LIMB_COUNT
    }
}
