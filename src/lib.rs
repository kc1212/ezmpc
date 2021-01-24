pub mod algebra;
pub mod crypto;
pub mod error;
pub mod message;
pub mod party;
pub mod synchronizer;
pub mod vm;

#[cfg(test)]
mod integration_test;

extern crate auto_ops;
extern crate crossbeam_channel;
extern crate log;
extern crate rand;
extern crate thiserror;

extern crate alga;
extern crate alga_derive;
extern crate num_traits;

#[cfg(test)]
extern crate itertools;
#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;
#[cfg(test)]
extern crate test_env_log;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("NTL/ZZ.h");
        include!("NTL/ZZ_p.h");
        include!("shim.h");

        #[namespace = "NTL"]
        type ZZ;
        fn ZZ_from_i64(a: i64) -> UniquePtr<ZZ>;
        fn ZZ_from_str(a: &str) -> UniquePtr<ZZ>;
        fn ZZ_add(a: &UniquePtr<ZZ>, b: &UniquePtr<ZZ>) -> UniquePtr<ZZ>;
        fn ZZ_to_string(z: &UniquePtr<ZZ>) -> String;

        #[namespace = "NTL"]
        type ZZ_p;
        fn ZZ_p_init(p: &UniquePtr<ZZ>);
        fn ZZ_p_zero() -> UniquePtr<ZZ_p>;
        fn ZZ_p_clone(p: &UniquePtr<ZZ_p>) -> UniquePtr<ZZ_p>;
        fn ZZ_p_from_i64(a: i64) -> UniquePtr<ZZ_p>;
        fn ZZ_p_from_str(a: &str) -> UniquePtr<ZZ_p>;
        fn ZZ_p_neg(a: &UniquePtr<ZZ_p>) -> UniquePtr<ZZ_p>;
        fn ZZ_p_inv(a: &UniquePtr<ZZ_p>) -> UniquePtr<ZZ_p>;
        fn ZZ_p_add(a: &UniquePtr<ZZ_p>, b: &UniquePtr<ZZ_p>) -> UniquePtr<ZZ_p>;
        fn ZZ_p_add_assign(a: &mut UniquePtr<ZZ_p>, b: &UniquePtr<ZZ_p>);
        fn ZZ_p_sub(a: &UniquePtr<ZZ_p>, b: &UniquePtr<ZZ_p>) -> UniquePtr<ZZ_p>;
        fn ZZ_p_sub_assign(a: &mut UniquePtr<ZZ_p>, b: &UniquePtr<ZZ_p>);
        fn ZZ_p_mul(a: &UniquePtr<ZZ_p>, b: &UniquePtr<ZZ_p>) -> UniquePtr<ZZ_p>;
        fn ZZ_p_mul_assign(a: &mut UniquePtr<ZZ_p>, b: &UniquePtr<ZZ_p>);
        fn ZZ_p_div(a: &UniquePtr<ZZ_p>, b: &UniquePtr<ZZ_p>) -> UniquePtr<ZZ_p>;
        fn ZZ_p_div_assign(a: &mut UniquePtr<ZZ_p>, b: &UniquePtr<ZZ_p>);
        fn ZZ_p_to_string(z: &UniquePtr<ZZ_p>) -> String;
        fn ZZ_p_eq(a: &UniquePtr<ZZ_p>, b: &UniquePtr<ZZ_p>) -> bool;
        fn ZZ_p_to_bytes(a: &UniquePtr<ZZ_p>) -> Vec<u8>;
        fn ZZ_p_from_bytes(a: &Vec<u8>) -> UniquePtr<ZZ_p>;
    }
}

mod zzp {
    use super::ffi::*;

    use alga::general::{AbstractMagma, Additive, Identity, Multiplicative, TwoSidedInverse};
    use alga_derive::Alga;
    use std::ops::{AddAssign, DivAssign, MulAssign, Neg, SubAssign};
    use num_traits::{One, Zero};
    use auto_ops::*;
    use std::fmt;
    use cxx;

    #[derive(Alga)]
    #[alga_traits(Field(Additive, Multiplicative))]
    pub struct Fp(cxx::UniquePtr<ZZ_p>); // we can only hold 64-bit values

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
        AbstractMagma::<Additive>::operate(a, b)
    });

    impl_op_ex!(-|a: &Fp, b: &Fp| -> Fp { AbstractMagma::<Additive>::operate(a, &TwoSidedInverse::<Additive>::two_sided_inverse(&b)) });

    impl AddAssign<Fp> for Fp {
        fn add_assign(&mut self, rhs: Fp) {
            ZZ_p_add_assign(&mut self.0, &rhs.0)
        }
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
        a * TwoSidedInverse::<Multiplicative>::two_sided_inverse(b)
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
}

#[cfg(test)]
mod test {
    use super::ffi::*;
    use super::zzp::*;
    use alga::general::Field;

    #[test]
    fn zz_p() {
        let p = ZZ_from_i64(91);
        ZZ_p_init(&p);

        let a = ZZ_p_from_i64(88);
        let one = ZZ_p_from_i64(1);
        let two = ZZ_p_from_i64(2);
        let three = ZZ_p_from_i64(3);
        assert!(ZZ_p_eq(&ZZ_p_add(&a, &a), &ZZ_p_mul(&a, &two)));
        assert!(!ZZ_p_eq(&ZZ_p_add(&a, &a), &ZZ_p_mul(&a, &three)));

        let mut mut_two = ZZ_p_from_i64(2);
        ZZ_p_add_assign(&mut mut_two, &one);
        assert!(ZZ_p_eq(&mut_two, &three));

        let a_str = ZZ_p_to_bytes(&a);
        assert!(ZZ_p_eq(&ZZ_p_from_bytes(&a_str), &a));
    }

    #[test]
    fn test_trait_impl() {
        fn is_field<T: Field>() {}
        is_field::<Fp>();
    }
}
