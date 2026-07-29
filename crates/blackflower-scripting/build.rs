use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const LUAU_BUILD: &str = "vendor/luau/CMakeLists.txt";
const LUAU_HEADER: &str = "vendor/luau/VM/include/lua.h";
const LUAU_COMPILER_HEADER: &str = "vendor/luau/Compiler/include/luacode.h";
const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";

fn main() -> Result<(), Box<dyn Error>> {
    for path in [
        LUAU_BUILD,
        LUAU_HEADER,
        LUAU_COMPILER_HEADER,
        NATIVE_BUILD,
        WRAPPER_HEADER,
        WRAPPER_SOURCE,
    ] {
        println!("cargo:rerun-if-changed={path}");
        require_file(path)?;
    }
    println!("cargo:rerun-if-changed=vendor/luau/Ast");
    println!("cargo:rerun-if-changed=vendor/luau/Bytecode");
    println!("cargo:rerun-if-changed=vendor/luau/Common");
    println!("cargo:rerun-if-changed=vendor/luau/Compiler");
    println!("cargo:rerun-if-changed=vendor/luau/VM");

    let install_dir = compile_native();
    generate_bindings()?;
    link_native(&install_dir)?;
    Ok(())
}

fn require_file(path: &str) -> Result<(), Box<dyn Error>> {
    if Path::new(path).is_file() {
        return Ok(());
    }

    Err(format!(
        "missing {path}; initialize the Luau submodule with \
         `git submodule update --init --recursive`"
    )
    .into())
}

fn compile_native() -> PathBuf {
    let mut config = cmake::Config::new("native");
    config.profile("Release").define("LUAU_WERROR", "OFF");
    config.build()
}

fn generate_bindings() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .clang_arg("-DLUA_USE_LONGJMP=1")
        .clang_arg("-Ivendor/luau/VM/include")
        .allowlist_function("^(bf_scripting_|lua_|luaL_|luau_).*")
        .allowlist_type("^(BFScripting|lua_).*")
        .allowlist_var("^(BF_SCRIPTING_|LUA_).*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for Luau bindings; install libclang and set \
         LIBCLANG_PATH to the directory containing the shared library"
    })?;
    let bindings = generation.map_err(|error| {
        format!(
            "failed to generate Luau bindings; install libclang and set \
             LIBCLANG_PATH if it is not discoverable: {error}"
        )
    })?;

    bindings.write_to_file(out_dir.join("luau_bindings.rs"))?;
    Ok(())
}

fn link_native(install_dir: &Path) -> Result<(), Box<dyn Error>> {
    for directory in ["lib", "lib64"] {
        let path = install_dir.join(directory);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    for library in [
        "blackflower_scripting_wrapper",
        "blackflower_luau_compiler",
        "blackflower_luau_ast",
        "blackflower_luau_bytecode",
        "blackflower_luau_vm",
        "blackflower_luau_common",
    ] {
        println!("cargo:rustc-link-lib=static={library}");
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    let target_env = env::var("CARGO_CFG_TARGET_ENV")?;
    match target_os.as_str() {
        "linux" | "android" => println!("cargo:rustc-link-lib=stdc++"),
        "macos" | "ios" | "freebsd" => println!("cargo:rustc-link-lib=c++"),
        "windows" if target_env == "gnu" => println!("cargo:rustc-link-lib=stdc++"),
        _ => {}
    }
    Ok(())
}
