#include "shim.h"
#include <sstream>
#include <algorithm>

ZZ_pContext GLOBAL_ZZ_p_CTX;

// ZZ

std::unique_ptr<ZZ> ZZ_from_i64(long a) {
	return std::make_unique<ZZ>(a);
}

std::unique_ptr<ZZ> ZZ_from_str(rust::Str a) {
	ZZ z;
	conv(z, a.data());
	return std::make_unique<ZZ>(z);
}

rust::String ZZ_to_string(const std::unique_ptr<ZZ> &z) {
	std::stringstream ss;
	ss << *z;
	return rust::String(ss.str());
}

long ZZ_num_bytes(const std::unique_ptr<ZZ> &z) {
	return NumBytes(*z);
}

// ZZ_p

void ZZ_p_init(const std::unique_ptr<ZZ> &a) {
	ZZ_p::init(*a);
}

std::unique_ptr<ZZ_p> ZZ_p_zero() {
	return std::make_unique<ZZ_p>(ZZ_p::zero());
}

std::unique_ptr<ZZ_p> ZZ_p_clone(const std::unique_ptr<ZZ_p> &z) {
	ZZ_p out(*z);
	return std::make_unique<ZZ_p>(out);
}

std::unique_ptr<ZZ_p> ZZ_p_from_i64(long a) {
	return std::make_unique<ZZ_p>(a);
}

std::unique_ptr<ZZ_p> ZZ_p_from_str(rust::Str a) {
	ZZ_p z;
	conv(z, a.data());
	return std::make_unique<ZZ_p>(z);
}

std::unique_ptr<ZZ_p> ZZ_p_neg(const std::unique_ptr<ZZ_p> &a) {
	return std::make_unique<ZZ_p>(-(*a));
}

std::unique_ptr<ZZ_p> ZZ_p_inv(const std::unique_ptr<ZZ_p> &a) {
	return std::make_unique<ZZ_p>(inv(*a));
}

std::unique_ptr<ZZ_p> ZZ_p_add(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b) {
	return std::make_unique<ZZ_p>(*a + *b);
}

void ZZ_p_add_assign(std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b) {
	add(*a, *a, *b);
}

std::unique_ptr<ZZ_p> ZZ_p_sub(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b) {
	return std::make_unique<ZZ_p>(*a - *b);
}

void ZZ_p_sub_assign(std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b) {
	sub(*a, *a, *b);
}

std::unique_ptr<ZZ_p> ZZ_p_mul(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b) {
    return std::make_unique<ZZ_p>((*a) * (*b));
}

void ZZ_p_mul_assign(std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b) {
	mul(*a, *a, *b);
}

std::unique_ptr<ZZ_p> ZZ_p_div(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b) {
    return std::make_unique<ZZ_p>((*a) / (*b));
}

void ZZ_p_div_assign(std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b) {
	div(*a, *a, *b);
}

rust::String ZZ_p_to_string(const std::unique_ptr<ZZ_p> &z) {
	std::stringstream ss;
	ss << *z;
	return rust::String(ss.str());
}
bool ZZ_p_eq(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b) {
    return *a == *b;
}

rust::Vec<unsigned char> ZZ_p_to_bytes(const std::unique_ptr<ZZ_p> &a) {
	auto n = NumBytes(rep(*a));
	std::vector<unsigned char> out;
	out.resize(n);
	BytesFromZZ(reinterpret_cast<unsigned char *>(&out[0]), rep(*a), n);

	// is there a better way to do this conversion?
	rust::Vec<unsigned char> rust_vec;
	rust_vec.reserve(n);
	std::copy(out.begin(), out.end(), std::back_inserter(rust_vec));
	return rust_vec;
}

std::unique_ptr<ZZ_p> ZZ_p_from_bytes(const rust::Vec<unsigned char> &s) {
	ZZ_p z;
	conv(z, ZZFromBytes(reinterpret_cast<const unsigned char *>(s.data()), s.size()));
	return std::make_unique<ZZ_p>(z);
}

void ZZ_p_save_context_global() {
	GLOBAL_ZZ_p_CTX.save();
}

void ZZ_p_restore_context_global() {
	GLOBAL_ZZ_p_CTX.restore();
}

std::unique_ptr<ZZ> ZZ_p_modulus() {
	return std::make_unique<ZZ>(ZZ_p::modulus());
}
