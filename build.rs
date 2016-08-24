use ::std::env;

fn main() {
    let target = env::var("TARGET").unwrap();

    if target.contains("windows") {
        // we need this stuff for rust >= 1.11 and nanomsg >= 0.5.0
        println!("cargo:rustc-link-search=c:/Windows/System32");
        println!("cargo:rustc-link-lib=mswsock");
    }
}
