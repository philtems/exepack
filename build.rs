use std::fs;
use std::path::Path;

fn main() {
    // Create output directory for decompressors
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let decompressors_dir = Path::new(&out_dir).join("decompressors");
    fs::create_dir_all(&decompressors_dir).unwrap();

    // Copy TEMS XZ decompressor from external folder
    let temsxz_decomp_path = Path::new("decompressors/temsxz");
    let temsxz_decomp_dest = decompressors_dir.join("decompress_temsxz.bin");
    
    if temsxz_decomp_path.exists() {
        fs::copy(temsxz_decomp_path, temsxz_decomp_dest).expect("Failed to copy temsxz");
        println!("cargo:warning=TEMS XZ decompressor copied successfully");
    } else {
        println!("cargo:warning=TEMS XZ decompressor not found at {}", temsxz_decomp_path.display());
        println!("cargo:warning=Please compile temsxz manually and place it in decompressors/");
    }
    
    println!("cargo:rerun-if-changed=decompressors/temsxz");
}

