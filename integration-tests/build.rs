use ::std::path::PathBuf;

fn main() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("release");
    println!("cargo:rustc-link-search={}", path.display());
    println!("cargo:rustc-link-lib=turtl_core");
}
