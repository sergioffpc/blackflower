use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const LUAU_VERSION: &str = "0.731.0";
const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";

fn main() -> Result<(), Box<dyn Error>> {
    for path in [NATIVE_BUILD, WRAPPER_HEADER, WRAPPER_SOURCE] {
        println!("cargo:rerun-if-changed={path}");
        require_file(path)?;
    }
    blackflower_build::emit_rerun_environment();
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let (configuration, workspace_root, luau) =
        blackflower_build::locate_from_cargo_build_script(&manifest_dir, "luau", LUAU_VERSION)
            .map_err(blackflower_build_error)?;
    let luau_source = workspace_root.join("vendor/luau");
    let libraries = load_luau_libraries(&luau, &configuration)?;

    let install_dir = compile_wrapper(&configuration, &luau_source, &libraries);
    generate_bindings(&luau_source)?;
    link_native(&install_dir, &libraries)?;
    Ok(())
}

struct LuauLibraries {
    compiler: PathBuf,
    ast: PathBuf,
    bytecode: PathBuf,
    codegen: PathBuf,
    vm: PathBuf,
    common: PathBuf,
}

fn load_luau_libraries(
    root: &Path,
    configuration: &blackflower_build::Configuration,
) -> Result<LuauLibraries, Box<dyn Error>> {
    let find = |name: &str| {
        blackflower_build::find_static_library(root, configuration, name, name)
            .map_err(blackflower_build_error)
    };
    Ok(LuauLibraries {
        compiler: find("blackflower_luau_compiler")?,
        ast: find("blackflower_luau_ast")?,
        bytecode: find("blackflower_luau_bytecode")?,
        codegen: find("blackflower_luau_codegen")?,
        vm: find("blackflower_luau_vm")?,
        common: find("blackflower_luau_common")?,
    })
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

fn compile_wrapper(
    configuration: &blackflower_build::Configuration,
    luau_source: &Path,
    libraries: &LuauLibraries,
) -> PathBuf {
    let mut config = cmake::Config::new("native");
    config
        .profile(configuration.cmake_profile)
        .static_crt(configuration.crt_static)
        .define("BLACKFLOWER_LUAU_ROOT", luau_source)
        .define("BLACKFLOWER_LUAU_COMPILER_LIBRARY", &libraries.compiler)
        .define("BLACKFLOWER_LUAU_AST_LIBRARY", &libraries.ast)
        .define("BLACKFLOWER_LUAU_BYTECODE_LIBRARY", &libraries.bytecode)
        .define("BLACKFLOWER_LUAU_CODEGEN_LIBRARY", &libraries.codegen)
        .define("BLACKFLOWER_LUAU_VM_LIBRARY", &libraries.vm)
        .define("BLACKFLOWER_LUAU_COMMON_LIBRARY", &libraries.common);
    config.build()
}

fn generate_bindings(luau_source: &Path) -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .clang_arg("-DLUA_USE_LONGJMP=1")
        .clang_arg(format!("-I{}", luau_source.join("VM/include").display()))
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

fn link_native(install_dir: &Path, libraries: &LuauLibraries) -> Result<(), Box<dyn Error>> {
    for directory in ["lib", "lib64"] {
        let path = install_dir.join(directory);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rustc-link-lib=static=blackflower_scripting_wrapper");
    for library in [
        &libraries.compiler,
        &libraries.ast,
        &libraries.bytecode,
        &libraries.codegen,
        &libraries.vm,
        &libraries.common,
    ] {
        blackflower_build::emit_static_library(library).map_err(blackflower_build_error)?;
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

fn blackflower_build_error(error: Box<dyn Error + Send + Sync>) -> std::io::Error {
    std::io::Error::other(error.to_string())
}
