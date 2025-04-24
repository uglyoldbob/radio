//https://github.com/clementb3/aasdk.git

fn init_submodule(path: &std::path::Path) {
    if !path.join("CMakeLists.txt").exists() {
        let _ = std::fs::remove_dir(&path);
        let mut builder = git2::build::RepoBuilder::new();
        let mut fo = git2::FetchOptions::new();
        fo.depth(1);
        builder.fetch_options(fo);
        builder
            .clone("https://github.com/clementb3/aasdk.git", &path)
            .expect("Failed to clone aasdk");
    }
}

fn get_os_from_triple(triple: &str) -> Option<&str> {
    triple.splitn(3, '-').nth(2)
}

fn compile(build_path: &std::path::Path, target_os: &str) -> std::path::PathBuf {
    let mut cfg = cmake::Config::new(build_path);
    if let Ok(profile) = std::env::var("AASDK_BUILD_PROFILE") {
        cfg.profile(&profile);
        cfg.define("CMAKE_BUILD_TYPE", &profile);
    } else {
        cfg.profile("Release");
        cfg.define("CMAKE_BUILD_TYPE", "Release");
    }
    cfg.build_target("aasdk");
    cfg.build()
}

fn main() {
    let p = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("aasdk");
    let target = std::env::var("TARGET").expect("Cargo build scripts always have TARGET");
    let target_os = get_os_from_triple(target.as_str()).unwrap();
    init_submodule(p.as_path());
    let compiled_path = compile(p.as_path(), target_os);
    println!("Compile path is {}", compiled_path.display());
    let mut a = cxx_build::bridge("src/aasdk.rs");
    a.file(p.join("src/Common/Data.cpp"))
        .include(p.join("include"))
        .include(compiled_path.join("build"))
        .std("c++14")
        .compile("aasdk-test");
    println!("cargo:rerun-if-changed=src/aasdk.rs");

    println!(
        "cargo:rustc-link-search={}",
        compiled_path.join("lib64").display()
    );
    println!(
        "cargo:rustc-link-search={}",
        compiled_path.join("lib").display()
    );
}
