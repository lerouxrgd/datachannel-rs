use std::env;
use std::path::PathBuf;

fn env_var_rerun(name: &str) -> Result<String, env::VarError> {
    println!("cargo:rerun-if-env-changed={}", name);
    env::var(name)
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    if cfg!(feature = "static") {
        let mut config = cmake::Config::new("libdatachannel");
        config.build_target("datachannel-static");
        config.out_dir(&out_dir);
        config.define("NO_WEBSOCKET", "ON");
        config.define("NO_EXAMPLES", "ON");
        config.define("NO_MEDIA", "ON");
        config.build();
    }

    let mut config = cmake::Config::new("libdatachannel");
    config.out_dir(&out_dir);
    config.define("NO_WEBSOCKET", "ON");
    config.define("NO_EXAMPLES", "ON");

    if let Ok(openssl_root_dir) = env_var_rerun("OPENSSL_ROOT_DIR") {
        config.define("OPENSSL_ROOT_DIR", openssl_root_dir);
    }

    if let Ok(openssl_libraries) = env_var_rerun("OPENSSL_LIBRARIES") {
        config.define("OPENSSL_LIBRARIES", openssl_libraries);
    }

    config.build();

    if cfg!(feature = "static") {
        // Link static libc++
        #[cfg(feature = "static")]
        cpp_build::Config::new()
            .include(format!("{}/lib", out_dir))
            .build("src/lib.rs");

        // Link static openssl
        println!("cargo:rustc-link-lib=static=crypto");
        println!("cargo:rustc-link-lib=static=ssl");

        // Link static libjuice
        println!(
            "cargo:rustc-link-search=native={}/build/deps/libjuice",
            out_dir
        );
        println!("cargo:rustc-link-lib=static=juice-static");

        // Link static usrsctplib
        println!(
            "cargo:rustc-link-search=native={}/build/deps/usrsctp/usrsctplib",
            out_dir
        );
        println!("cargo:rustc-link-lib=static=usrsctp");

        // Link static libdatachannel
        println!("cargo:rustc-link-search=native={}/build", out_dir);
        println!("cargo:rustc-link-lib=static=datachannel-static");
    } else {
        // Link dynamic libdatachannel
        println!("cargo:rustc-link-search=native={}/lib", out_dir);
        println!("cargo:rustc-link-lib=dylib=datachannel");
    }

    let bindings = bindgen::Builder::default()
        .header(format!("{}/include/rtc/rtc.h", out_dir))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(out_dir);
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings");
}
