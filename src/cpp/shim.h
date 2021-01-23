// https://github.com/dtolnay/cxx/issues/221
// https://github.com/dtolnay/cxx/issues/280

#pragma once
#include <memory>
#include "NTL/ZZ.h"
#include "NTL/ZZ_p.h"
#include "rust/cxx.h"

using namespace NTL;

std::unique_ptr<ZZ> ZZ_from_i64(long a);
std::unique_ptr<ZZ> ZZ_from_str(rust::Str a);
std::unique_ptr<ZZ> ZZ_add(const std::unique_ptr<ZZ> &a, const std::unique_ptr<ZZ> &b);
rust::String ZZ_to_string(const std::unique_ptr<ZZ> &z);

void ZZ_p_init(const std::unique_ptr<ZZ> &z);
std::unique_ptr<ZZ_p> ZZ_p_from_i64(long a);
std::unique_ptr<ZZ_p> ZZ_p_from_str(rust::Str a);
std::unique_ptr<ZZ_p> ZZ_p_add(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b);
std::unique_ptr<ZZ_p> ZZ_p_mul(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b);
rust::String ZZ_p_to_string(const std::unique_ptr<ZZ_p> &z);
bool ZZ_p_eq(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b);
