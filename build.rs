fn main() {
    if std::env::var("CARGO_FEATURE_IMXVPUAPI2").is_ok() {
        println!("cargo:rustc-link-lib=imxvpuapi2");
        println!("cargo:rustc-link-lib=imxdmabuffer");
        println!("cargo:rerun-if-env-changed=CARGO_FEATURE_IMXVPUAPI2");
    }
}
