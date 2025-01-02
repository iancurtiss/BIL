use std::env::{ self };
use std::path::PathBuf;

fn main() {
    println!("env: {:?}", env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let cargo_manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let archive_path = match env::var_os("MINER_WASM_PATH") {
        Some(wasm_path) => PathBuf::from(wasm_path),
        None => {
            let project_root = cargo_manifest_dir.join("../..").canonicalize().unwrap();
            project_root.join(".dfx/local/canisters/windoge_miner/windoge_miner.wasm.gz")
        }
    };

    println!("cargo:rerun-if-changed={}", archive_path.display());
    println!("cargo:rerun-if-env-changed=MINER_WASM_PATH");
    println!("cargo:rustc-env=MINER_WASM_PATH={}", archive_path.display());
}
