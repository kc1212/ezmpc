// https://github.com/dtolnay/cxx/issues/221
// https://github.com/dtolnay/cxx/issues/280

#pragma once
#include <memory>
#include <vector>
#include "NTL/ZZ.h"
#include "NTL/ZZ_p.h"
#include "rust/cxx.h"

using namespace NTL;

std::unique_ptr<ZZ> ZZ_from_i64(long a);
std::unique_ptr<ZZ> ZZ_from_str(rust::Str a);
rust::String ZZ_to_string(const std::unique_ptr<ZZ> &z);
long ZZ_num_bytes(const std::unique_ptr<ZZ> &z);

void ZZ_p_init(const std::unique_ptr<ZZ> &z);
std::unique_ptr<ZZ_p> ZZ_p_zero();
std::unique_ptr<ZZ_p> ZZ_p_clone(const std::unique_ptr<ZZ_p> &z);
std::unique_ptr<ZZ_p> ZZ_p_from_i64(long a);
std::unique_ptr<ZZ_p> ZZ_p_from_str(rust::Str a);
std::unique_ptr<ZZ_p> ZZ_p_neg(const std::unique_ptr<ZZ_p> &a);
std::unique_ptr<ZZ_p> ZZ_p_inv(const std::unique_ptr<ZZ_p> &a);
std::unique_ptr<ZZ_p> ZZ_p_add(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b);
void ZZ_p_add_assign(std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b);
std::unique_ptr<ZZ_p> ZZ_p_sub(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b);
void ZZ_p_sub_assign(std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b);
std::unique_ptr<ZZ_p> ZZ_p_mul(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b);
void ZZ_p_mul_assign(std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b);
std::unique_ptr<ZZ_p> ZZ_p_div(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b);
void ZZ_p_div_assign(std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b);
rust::String ZZ_p_to_string(const std::unique_ptr<ZZ_p> &z);
bool ZZ_p_eq(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b);
rust::Vec<unsigned char> ZZ_p_to_bytes(const std::unique_ptr<ZZ_p> &a);
std::unique_ptr<ZZ_p> ZZ_p_from_bytes(const rust::Vec<unsigned char> &s);

void ZZ_p_save_context_global();
void ZZ_p_restore_context_global();
std::unique_ptr<ZZ> ZZ_p_modulus();
