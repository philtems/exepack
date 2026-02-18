use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=decompressors/");
    
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_dir = Path::new(&out_dir).join("decompressors");
    fs::create_dir_all(&dest_dir).unwrap();
    
    // Compiler les décompresseurs C avec make
    let status = Command::new("make")
        .args(&["-C", "decompressors"])
        .status()
        .expect("Échec de la compilation des décompresseurs");
    
    if !status.success() {
        panic!("La compilation des décompresseurs a échoué");
    }
    
    // Copier les binaires compilés
    for algo in &["gzip", "zstd", "xz"] {
        let src = format!("decompressors/decompress_{}.bin", algo);
        let dst = dest_dir.join(format!("decompress_{}.bin", algo));
        if Path::new(&src).exists() {
            fs::copy(&src, &dst).expect("Échec copie décompresseur");
            println!("cargo:warning=Décompresseur {} copié ({} octets)", 
                     algo, fs::metadata(&src).unwrap().len());
        } else {
            panic!("Décompresseur {} non trouvé", algo);
        }
    }
    
    println!("cargo:rustc-env=OUT_DIR={}", out_dir);
    println!("cargo:warning=Tous les décompresseurs C ont été compilés avec succès");
}

