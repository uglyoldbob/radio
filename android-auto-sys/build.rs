//https://github.com/clementb3/aasdk.git

fn init_submodule(path: &std::path::Path) {
    if !path.join("CMakeLists.txt").exists() {
        let mut path = std::path::PathBuf::from(path.clone());
        path.pop();
        let _ = std::fs::remove_dir(&path);
        std::process::Command::new("git")
            .args(["clone", "https://github.com/clementb3/aasdk.git"])
            .current_dir(&path)
            .status()
            .expect("Git is needed to retrieve the source files");
    }
}

fn get_os_from_triple(triple: &str) -> Option<&str> {
    triple.splitn(3, '-').nth(2)
}

fn compile(build_path: &std::path::Path, target_os: &str) -> std::path::PathBuf {
    let mut cfg = cmake::Config::new(build_path);
    if let Ok(profile) = std::env::var("AASDK_BUILD_PROFILE") {
        cfg.profile(&profile);
        cfg.define("CMAKE_CONFIGURATION_TYPES", &profile);
    } else {
        cfg.profile("Release");
        cfg.define("CMAKE_CONFIGURATION_TYPES", "Release");
    }

    cfg.build()
}

fn main() {
    let p = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("aasdk");
    let target = std::env::var("TARGET").expect("Cargo build scripts always have TARGET");
    let target_os = get_os_from_triple(target.as_str()).unwrap();
    init_submodule(p.as_path());
    let compiled_path = compile(p.as_path(), target_os);

    println!(
        "cargo:rustc-link-search={}",
        compiled_path.join("lib64").display()
    );
    println!(
        "cargo:rustc-link-search={}",
        compiled_path.join("lib").display()
    );
}