use auto_ops::*;
use base64;
use ff::*;
use num_traits::{One, Zero};
use quickcheck::{Arbitrary, Gen};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::mem::transmute;
use std::ops::*;
use std::str::FromStr;

#[derive(PrimeField, Serialize, Deserialize)]
#[PrimeFieldModulus = "52435875175126190479447740508185965837690552500527637822603658699938581184513"]
#[PrimeFieldGenerator = "7"]
#[PrimeFieldReprEndianness = "little"]
struct InnerFp([u64; 4]);
const LIMB_SIZE: usize = 4;
const FP_BYTES: usize = 64 * LIMB_SIZE / 8;

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

fn to_vec_u8(fp: &Fp) -> Vec<u8> {
    let mut out = vec![];
    out.reserve_exact(FP_BYTES);
    for i in 0..LIMB_SIZE {
        unsafe {
            out.extend_from_slice(&transmute::<u64, [u8; 8]>(fp.0 .0[i]));
        }
    }
    out
}

fn from_vec_u8(v: &Vec<u8>) -> Result<Fp, base64::DecodeError> {
    let mut out = [0u64; LIMB_SIZE];
    for i in 0..LIMB_SIZE {
        let mut u64_bytes = [0u8; 8];
        u64_bytes.copy_from_slice(&v[i * 8..(i + 1) * 8]);
        out[i] = u64::from_le_bytes(u64_bytes);
    }
    Ok(Fp(InnerFp(out)))
}

impl ToString for Fp {
    fn to_string(&self) -> String {
        base64::encode(to_vec_u8(self))
    }
}

impl FromStr for Fp {
    type Err = base64::DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(from_vec_u8(&base64::decode(s)?)?)
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
    fn prop_string(x: Fp) -> bool {
        Fp::from_str(&x.to_string()).unwrap() == x
    }

    #[quickcheck]
    fn prop_limb_size(x: Fp) -> bool {
        x.0 .0.len() == LIMB_SIZE
    }
}
