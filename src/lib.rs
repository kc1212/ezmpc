pub mod algebra;
pub mod crypto;
pub mod error;
pub mod message;
pub mod party;
pub mod synchronizer;
pub mod vm;
pub mod net;

#[cfg(test)]
mod integration_test;

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
        fn ZZ_to_string(z: &UniquePtr<ZZ>) -> String;
        fn ZZ_num_bytes(z: &UniquePtr<ZZ>) -> i64;

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
        fn ZZ_p_from_bytes(a: &[u8]) -> UniquePtr<ZZ_p>;

        fn ZZ_p_save_context_global();
        fn ZZ_p_restore_context_global();
        fn ZZ_p_modulus() -> UniquePtr<ZZ>;
    }
}
