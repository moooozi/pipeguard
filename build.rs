use rand::{RngCore, SeedableRng};
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() {
    // Generate a random 32-byte key at compile time
    let mut rng = rand::rngs::StdRng::from_entropy();
    let mut key = [0u8; 32];
    rng.fill_bytes(&mut key);

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("default_key.rs");
    let mut f = File::create(&dest_path).unwrap();

    // Write the default key as a const
    write!(
        f,
        "// This key is generated at compile time for secure default encryption\n"
    )
    .unwrap();
    write!(
        f,
        "pub(crate) const DEFAULT_ENCRYPTION_KEY: [u8; 32] = {:?};\n",
        key
    )
    .unwrap();

    println!("cargo:rerun-if-changed=build.rs");
}
