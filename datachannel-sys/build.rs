use std::env;
use std::path::PathBuf;

#[cfg(feature = "vendored")]
use once_cell::sync::OnceCell;

#[allow(dead_code)]
fn env_var_rerun(name: &str) -> Result<String, env::VarError> {
    println!("cargo:rerun-if-env-changed={}", name);
    env::var(name)
}

#[cfg(feature = "vendored")]
pub fn openssl_artifacts() -> &'static openssl_src::Artifacts {
    static INSTANCE: OnceCell<openssl_src::Artifacts> = OnceCell::new();
    INSTANCE.get_or_init(|| openssl_src::Build::new().build())
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    #[cfg(feature = "vendored")]
    {
        let mut cmake_conf = cmake::Config::new("libdatachannel");
        cmake_conf.build_target("datachannel-static");
        cmake_conf.out_dir(&out_dir);

        cmake_conf.define("CMAKE_POLICY_VERSION_MINIMUM", "3.5");
        cmake_conf.define("NO_WEBSOCKET", "ON");
        cmake_conf.define("NO_EXAMPLES", "ON");
        if !cfg!(feature = "media") {
            cmake_conf.define("NO_MEDIA", "ON");
        }

        let openssl_root_dir = openssl_artifacts().lib_dir().parent().unwrap();
        cmake_conf.define("OPENSSL_ROOT_DIR", openssl_root_dir.to_path_buf());
        cmake_conf.define(
            "OPENSSL_INCLUDE_DIR",
            openssl_root_dir.to_path_buf().join("include"),
        );
        cmake_conf.define(
            "OPENSSL_CRYPTO_LIBRARY",
            openssl_root_dir.to_path_buf().join("lib/libcrypto.a"),
        );
        cmake_conf.define(
            "OPENSSL_SSL_LIBRARY",
            openssl_root_dir.to_path_buf().join("lib/libssl.a"),
        );
        cmake_conf.define("OPENSSL_USE_STATIC_LIBS", "TRUE");

        cmake_conf.build();

        let profile = cmake_conf.get_profile();

        // Link static libc++
        cpp_build::Config::new()
            .include(format!("{}/lib", out_dir))
            .build("src/lib.rs");

        // Link static openssl
        println!(
            "cargo:rustc-link-search=native={}",
            openssl_artifacts().lib_dir().to_path_buf().display()
        );
        if cfg!(target_env = "msvc") {
            println!("cargo:rustc-link-lib=static=libcrypto");
            println!("cargo:rustc-link-lib=static=libssl");
        } else {
            println!("cargo:rustc-link-lib=static=crypto");
            println!("cargo:rustc-link-lib=static=ssl");
        }

        // Link static libjuice
        if cfg!(target_env = "msvc") {
            println!(
                "cargo:rustc-link-search=native={}/build/deps/libjuice/{}",
                out_dir, profile
            );
        } else {
            println!(
                "cargo:rustc-link-search=native={}/build/deps/libjuice",
                out_dir
            );
        }
        println!("cargo:rustc-link-lib=static=juice-static");

        // Link static usrsctplib
        if cfg!(target_env = "msvc") {
            println!(
                "cargo:rustc-link-search=native={}/build/deps/usrsctp/usrsctplib/{}",
                out_dir, profile
            );
        } else {
            println!(
                "cargo:rustc-link-search=native={}/build/deps/usrsctp/usrsctplib",
                out_dir
            );
        }
        println!("cargo:rustc-link-lib=static=usrsctp");

        if cfg!(feature = "media") {
            // Link static libsrtp
            if cfg!(target_env = "msvc") {
                println!(
                    "cargo:rustc-link-search=native={}/build/deps/libsrtp/{}",
                    out_dir, profile
                );
            } else {
                println!(
                    "cargo:rustc-link-search=native={}/build/deps/libsrtp",
                    out_dir
                );
            }
            println!("cargo:rustc-link-lib=static=srtp2");
        }

        // Link static libdatachannel
        if cfg!(target_env = "msvc") {
            println!(
                "cargo:rustc-link-search=native={}/build/{}",
                out_dir, profile
            );
        } else {
            println!("cargo:rustc-link-search=native={}/build", out_dir);
        }
        println!("cargo:rustc-link-lib=static=datachannel-static");
    }

    #[cfg(not(feature = "vendored"))]
    {
        let mut cmake_conf = cmake::Config::new("libdatachannel");
        cmake_conf.out_dir(&out_dir);

        cmake_conf.define("CMAKE_POLICY_VERSION_MINIMUM", "3.5");
        cmake_conf.define("NO_WEBSOCKET", "ON");
        cmake_conf.define("NO_EXAMPLES", "ON");
        if !cfg!(feature = "media") {
            cmake_conf.define("NO_MEDIA", "ON");
        }

        if let Ok(openssl_root_dir) = env_var_rerun("OPENSSL_ROOT_DIR") {
            cmake_conf.define("OPENSSL_ROOT_DIR", openssl_root_dir);
        }
        if let Ok(openssl_libraries) = env_var_rerun("OPENSSL_LIBRARIES") {
            cmake_conf.define("OPENSSL_LIBRARIES", openssl_libraries);
        }

        cmake_conf.build();

        // Link dynamic libdatachannel
        println!("cargo:rustc-link-search=native={}/lib", out_dir);
        println!("cargo:rustc-link-lib=dylib=datachannel");
    }

    let bindings = bindgen::Builder::default()
        .header("libdatachannel/include/rtc/rtc.h")
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(out_dir);
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings");
}
