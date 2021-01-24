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
        fn ZZ_p_from_i64(a: i64) -> UniquePtr<ZZ_p>;
        fn ZZ_p_from_str(a: &str) -> UniquePtr<ZZ_p>;
        fn ZZ_p_add(a: &UniquePtr<ZZ_p>, b: &UniquePtr<ZZ_p>) -> UniquePtr<ZZ_p>;
        fn ZZ_p_add_assign(a: &mut UniquePtr<ZZ_p>, b: &UniquePtr<ZZ_p>);
        fn ZZ_p_mul(a: &UniquePtr<ZZ_p>, b: &UniquePtr<ZZ_p>) -> UniquePtr<ZZ_p>;
        fn ZZ_p_to_string(z: &UniquePtr<ZZ_p>) -> String;
        fn ZZ_p_eq(a: &UniquePtr<ZZ_p>, b: &UniquePtr<ZZ_p>) -> bool;
        fn ZZ_p_to_bytes(a: &UniquePtr<ZZ_p>) -> UniquePtr<CxxVector<u8>>;
        fn ZZ_p_from_bytes(a: &UniquePtr<CxxVector<u8>>) -> UniquePtr<ZZ_p>;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn zz_p() {
        let p = ffi::ZZ_from_i64(91);
        ffi::ZZ_p_init(&p);

        let a = ffi::ZZ_p_from_i64(88);
        let one = ffi::ZZ_p_from_i64(1);
        let two = ffi::ZZ_p_from_i64(2);
        let three = ffi::ZZ_p_from_i64(3);
        assert!(ffi::ZZ_p_eq(&ffi::ZZ_p_add(&a, &a), &ffi::ZZ_p_mul(&a, &two)));
        assert!(!ffi::ZZ_p_eq(&ffi::ZZ_p_add(&a, &a), &ffi::ZZ_p_mul(&a, &three)));

        let mut mut_two = ffi::ZZ_p_from_i64(2);
        ffi::ZZ_p_add_assign(&mut mut_two, &one);
        assert!(ffi::ZZ_p_eq(&mut_two, &three));

        let a_str = ffi::ZZ_p_to_bytes(&a);
        assert!(ffi::ZZ_p_eq(&ffi::ZZ_p_from_bytes(&a_str), &a));
    }
}
