use ::std::env;

fn main() {
    let target = env::var("TARGET").unwrap();

    if target.contains("windows") {
        println!("cargo:rustc-link-search=c:/Windows/System32");
        println!("cargo:rustc-link-search=c:/lib");
        println!("cargo:rustc-link-search=d:/msys2/usr/lib");
    }
}
