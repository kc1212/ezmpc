use alga::general::{AbstractMagma, Additive, Identity, Multiplicative, TwoSidedInverse};
use alga_derive::Alga;
use approx::{AbsDiffEq, RelativeEq};
use auto_ops::*;
use cxx;
use num_traits::{One, Zero};
use quickcheck::{Arbitrary, Gen};
use rand::{Rand, Rng};
use serde::de;
use serde::{Serialize, Serializer, Deserialize, Deserializer};
use rmp_serde;
use std::fmt;
use std::ops::{AddAssign, DivAssign, MulAssign, Neg, SubAssign};
use std::sync::Once;

use crate::ffi::*;

static INIT_P: Once = Once::new();
static P: &str = "340282366920938463463374607431768211297";

#[derive(Alga)]
#[alga_traits(Field(Additive, Multiplicative))]
#[alga_quickcheck]
pub struct Fp(cxx::UniquePtr<ZZ_p>);

impl Clone for Fp {
    fn clone(&self) -> Self {
        Fp(ZZ_p_clone(&self.0))
    }
}

impl PartialEq for Fp {
    fn eq(&self, other: &Self) -> bool {
        ZZ_p_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Fp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", ZZ_p_to_string(&self.0))
    }
}

impl Identity<Additive> for Fp {
    fn identity() -> Self {
        Fp(ZZ_p_zero())
    }
}

impl Identity<Multiplicative> for Fp {
    fn identity() -> Self {
        Fp(ZZ_p_from_i64(1))
    }
}

impl AbstractMagma<Additive> for Fp {
    fn operate(&self, right: &Self) -> Self {
        Fp(ZZ_p_add(&self.0, &right.0))
    }
}

impl AbstractMagma<Multiplicative> for Fp {
    fn operate(&self, right: &Self) -> Self {
        Fp(ZZ_p_mul(&self.0, &right.0))
    }
}

impl TwoSidedInverse<Additive> for Fp {
    fn two_sided_inverse(&self) -> Self {
        Fp(ZZ_p_neg(&self.0))
    }
}

impl TwoSidedInverse<Multiplicative> for Fp {
    fn two_sided_inverse(&self) -> Self {
        Fp(ZZ_p_inv(&self.0))
    }
}

impl_op_ex!(+|a: &Fp, b:  &Fp| -> Fp {
    Fp(ZZ_p_add(&a.0, &b.0)) // AbstractMagma::<Additive>::operate(a, b)
});

impl_op_ex!(-|a: &Fp, b: &Fp| -> Fp { 
    Fp(ZZ_p_sub(&a.0, &b.0))
});

pub fn ref_add_assign(lhs :&mut Fp, rhs: &Fp) {
    ZZ_p_add_assign(&mut lhs.0, &rhs.0)
}

impl AddAssign<Fp> for Fp {
    fn add_assign(&mut self, rhs: Fp) {
        ZZ_p_add_assign(&mut self.0, &rhs.0)
    }
}

pub fn ref_sub_assign(lhs :&mut Fp, rhs: &Fp) {
    ZZ_p_sub_assign(&mut lhs.0, &rhs.0)
}

impl SubAssign<Fp> for Fp {
    fn sub_assign(&mut self, rhs: Fp) {
        ZZ_p_sub_assign(&mut self.0, &rhs.0)
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
        ZZ_p_eq(&self.0, &ZZ_p_zero())
    }
}

impl One for Fp {
    fn one() -> Fp {
        Identity::<Multiplicative>::identity()
    }
}

impl_op_ex!(*|a: &Fp, b: &Fp| -> Fp { AbstractMagma::<Multiplicative>::operate(a, b) });

impl_op_ex!(/|a: &Fp, b: &Fp| -> Fp {
    Fp(ZZ_p_div(&a.0, &b.0))
});

impl MulAssign<Fp> for Fp {
    fn mul_assign(&mut self, rhs: Fp) {
        ZZ_p_mul_assign(&mut self.0, &rhs.0)
    }
}

impl DivAssign<Fp> for Fp {
    fn div_assign(&mut self, rhs: Fp) {
        ZZ_p_div_assign(&mut self.0, &rhs.0)
    }
}

impl AbsDiffEq for Fp {
    type Epsilon = Fp;
    fn default_epsilon() -> Fp {
        Fp::zero()
    }
    fn abs_diff_eq(&self, other: &Fp, _epsilon: Fp) -> bool {
        self == other
    }
}

impl RelativeEq for Fp {
    fn default_max_relative() -> Fp {
        Fp::zero()
    }

    fn relative_eq(&self, other: &Self, _epsilon: Fp, _max_relative: Fp) -> bool {
        self == other
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

pub fn init_or_restore_context() {
    INIT_P.call_once(|| {
        let p = ZZ_from_str(P);
        ZZ_p_init(&p);
        ZZ_p_save_context_global();
    });
    ZZ_p_restore_context_global()
}

unsafe impl Send for Fp {}
impl Arbitrary for Fp {
    fn arbitrary<G: Gen>(g: &mut G) -> Self {
        // we need to do this hack so that the modulus is initialized correctly
        init_or_restore_context();

        // TODO use i64 for now, write a proper Arbitrary later
        Fp(ZZ_p_from_i64(i64::arbitrary(g)))
    }
}

impl Serialize for Fp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where 
        S: Serializer
    {
        serializer.serialize_bytes(&ZZ_p_to_bytes(&self.0))
    }
}

struct FpVisitor;

impl<'de> de::Visitor<'de> for FpVisitor {
    type Value = Fp;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an field element")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where E: de::Error,
    {
        Ok(Fp(ZZ_p_from_bytes(v)))
    }
}

impl<'de> Deserialize<'de> for Fp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>
    {
        deserializer.deserialize_bytes(FpVisitor)
    }
}

impl Rand for Fp {
    fn rand<R: Rng>(rng: &mut R) -> Self {
        // TODO check if this is cryptographically secure
        let mut buf: Vec<u8> = Vec::new();
        buf.resize(ZZ_num_bytes(&ZZ_p_modulus()) as usize, 0);
        rng.fill_bytes(&mut buf);
        Fp(ZZ_p_from_bytes(&buf))
    }
}

impl From<i64> for Fp {
    fn from(x: i64) -> Self {
        Fp(ZZ_p_from_i64(x))
    }
}

pub fn get_modulus_string() -> String {
    ZZ_to_string(&ZZ_p_modulus())
}

#[cfg(test)]
mod test {
    use super::*;
    use alga::general::Field;
    
    #[test]
    fn test_modulus_string() {
        init_or_restore_context();
        assert_eq!(get_modulus_string(), P);
    }

    #[test]
    fn test_trait_impl() {
        init_or_restore_context();
        fn is_field<T: Field>() {}
        is_field::<Fp>();
    }
    
    #[quickcheck]
    fn prop_serialization(x: Fp) -> bool {
        // consider using serde_test crate
        let buf = rmp_serde::to_vec(&x).unwrap();
        x == rmp_serde::from_read_ref(&buf).unwrap()
    }
}
