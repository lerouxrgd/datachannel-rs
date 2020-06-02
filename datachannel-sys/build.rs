use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // let mut config = cmake::Config::new("libdatachannel");
    // config.build_target("datachannel-static");
    // config.out_dir(&out_dir);
    // config.define("USE_JUICE", "1");
    // config.define("USE_GNUTLS", "1");
    // config.build();

    let mut config = cmake::Config::new("libdatachannel");
    config.out_dir(&out_dir);
    config.define("USE_JUICE", "1");
    config.define("USE_GNUTLS", "1");
    config.build();

    println!("cargo:rustc-link-search=native={}/lib", out_dir);
    println!("cargo:rustc-link-lib=dylib=datachannel");

    let bindings = bindgen::Builder::default()
        .header(format!("{}/include/rtc/rtc.h", out_dir))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(out_dir);
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings");
}
