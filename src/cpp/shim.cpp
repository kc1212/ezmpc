#include "shim.h"
#include <sstream>

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

std::unique_ptr<ZZ> ZZ_add(const std::unique_ptr<ZZ> &a, const std::unique_ptr<ZZ> &b) {
	return std::make_unique<ZZ>(*a + *b);
}

// ZZ_p

void ZZ_p_init(const std::unique_ptr<ZZ> &a) {
	ZZ_p::init(*a);
}

std::unique_ptr<ZZ_p> ZZ_p_from_i64(long a) {
	return std::make_unique<ZZ_p>(a);
}

std::unique_ptr<ZZ_p> ZZ_p_from_str(rust::Str a) {
	ZZ_p z;
	conv(z, a.data());
	return std::make_unique<ZZ_p>(z);
}

std::unique_ptr<ZZ_p> ZZ_p_add(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b) {
	return std::make_unique<ZZ_p>(*a + *b);
}

std::unique_ptr<ZZ_p> ZZ_p_mul(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b) {
    return std::make_unique<ZZ_p>((*a) * (*b));
}

rust::String ZZ_p_to_string(const std::unique_ptr<ZZ_p> &z) {
	std::stringstream ss;
	ss << *z;
	return rust::String(ss.str());
}
bool ZZ_p_eq(const std::unique_ptr<ZZ_p> &a, const std::unique_ptr<ZZ_p> &b) {
    return *a == *b;
}
