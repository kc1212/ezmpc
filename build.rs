fn main() {
    cxx_build::bridge("src/lib.rs")
        .file("src/cpp/shim.cpp")
        .include("src/cpp")
        .flag_if_supported("-std=c++17")
        .compile("ezmpc");

    println!("cargo:rerun-if-changed=src/cpp/shim.cpp");
    println!("cargo:rerun-if-changed=src/cpp/shim.h");
    println!("cargo:rustc-link-lib=ntl");
}
