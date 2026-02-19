use std::env;
use std::fs;
use std::io::{self, Write, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process;

// Pour accéder aux métadonnées Unix
#[cfg(target_os = "linux")]
use std::os::linux::fs::MetadataExt;
#[cfg(target_os = "openbsd")]
use std::os::unix::fs::MetadataExt;

const MAGIC: &str = "# compressed by rust-gzexe\n";
const SCRIPT_RESERVED_SIZE: usize = 2000; // Espace réservé pour le script
const SIGNATURE: &str = "RUSTGZEXE:v1";   // Signature pour identifier nos fichiers

#[derive(Debug)]
enum CompressionAlgo {
    Gzip,
    Zstd,
    Xz,
}

impl CompressionAlgo {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "-gz" => Some(CompressionAlgo::Gzip),
            "-zstd" => Some(CompressionAlgo::Zstd),
            "-xz" => Some(CompressionAlgo::Xz),
            _ => None,
        }
    }

    fn to_str(&self) -> &'static str {
        match self {
            CompressionAlgo::Gzip => "gzip",
            CompressionAlgo::Zstd => "zstd",
            CompressionAlgo::Xz => "xz",
        }
    }

    fn decompressor_bin(&self) -> &'static [u8] {
        match self {
            CompressionAlgo::Gzip => include_bytes!(concat!(env!("OUT_DIR"), "/decompressors/decompress_gzip.bin")),
            CompressionAlgo::Zstd => include_bytes!(concat!(env!("OUT_DIR"), "/decompressors/decompress_zstd.bin")),
            CompressionAlgo::Xz => include_bytes!(concat!(env!("OUT_DIR"), "/decompressors/decompress_xz.bin")),
        }
    }

    fn compress(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        match self {
            CompressionAlgo::Gzip => {
                use flate2::write::GzEncoder;
                use flate2::Compression;
                let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
                encoder.write_all(data)?;
                encoder.finish()
            }
            CompressionAlgo::Zstd => {
                zstd::stream::encode_all(data, 22) // --ultra
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            }
            CompressionAlgo::Xz => {
                use xz2::write::XzEncoder;
                let mut encoder = XzEncoder::new(Vec::new(), 9);
                encoder.write_all(data)?;
                encoder.finish()
            }
        }
    }

    fn decompress(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        match self {
            CompressionAlgo::Gzip => {
                use flate2::read::GzDecoder;
                let mut decoder = GzDecoder::new(data);
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed)?;
                Ok(decompressed)
            }
            CompressionAlgo::Zstd => {
                zstd::stream::decode_all(data)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            }
            CompressionAlgo::Xz => {
                use xz2::read::XzDecoder;
                let mut decoder = XzDecoder::new(data);
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed)?;
                Ok(decompressed)
            }
        }
    }

    fn from_magic(data: &[u8]) -> Option<Self> {
        if data.starts_with(&[0x1f, 0x8b]) {
            Some(CompressionAlgo::Gzip)
        } else if data.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
            Some(CompressionAlgo::Zstd)
        } else if data.starts_with(&[0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00]) {
            Some(CompressionAlgo::Xz)
        } else {
            None
        }
    }
}

struct Config {
    algo: Option<CompressionAlgo>,
    decompress: bool,
    files: Vec<PathBuf>,
}

impl Config {
    fn parse_args() -> Result<Self, String> {
        let args: Vec<String> = env::args().collect();
        if args.len() < 2 {
            return Err("Usage: rust-gzexe [-gz | -zstd | -xz] [-d] <fichiers...>".to_string());
        }

        let mut config = Config {
            algo: None,
            decompress: false,
            files: Vec::new(),
        };

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "-gz" | "-zstd" | "-xz" => {
                    if config.algo.is_some() {
                        return Err("Un seul algorithme de compression peut être spécifié".to_string());
                    }
                    config.algo = CompressionAlgo::from_str(&args[i]);
                }
                "-d" => {
                    config.decompress = true;
                }
                arg if arg.starts_with('-') => {
                    return Err(format!("Option inconnue: {}", arg));
                }
                _ => {
                    config.files.push(PathBuf::from(&args[i]));
                }
            }
            i += 1;
        }

        if config.files.is_empty() {
            return Err("Aucun fichier spécifié".to_string());
        }

        if !config.decompress && config.algo.is_none() {
            config.algo = Some(CompressionAlgo::Gzip);
        }

        Ok(config)
    }
}

// Vérifier si le fichier est déjà compressé (avec notre signature)
fn is_compressed(path: &Path) -> io::Result<bool> {
    let mut file = fs::File::open(path)?;
    let mut buffer = [0u8; 256];
    let n = file.read(&mut buffer)?;
    
    if n < SIGNATURE.len() {
        return Ok(false);
    }
    
    // Chercher la signature dans les premiers caractères
    let content = String::from_utf8_lossy(&buffer[..n]);
    Ok(content.contains(SIGNATURE))
}

// Obtenir les bits setuid/setgid de façon portable
fn has_setuid_or_setgid(metadata: &fs::Metadata) -> bool {
    #[cfg(target_os = "linux")]
    {
        let mode = metadata.st_mode();
        (mode & 0o4000) != 0 || (mode & 0o2000) != 0
    }
    #[cfg(target_os = "openbsd")]
    {
        let mode = metadata.mode();
        (mode & 0o4000) != 0 || (mode & 0o2000) != 0
    }
    #[cfg(not(any(target_os = "linux", target_os = "openbsd")))]
    {
        false
    }
}

fn check_file(path: &Path) -> Result<fs::Metadata, String> {
    let metadata = fs::metadata(path)
        .map_err(|e| format!("Erreur lecture {}: {}", path.display(), e))?;

    if !metadata.is_file() {
        return Err(format!("{} n'est pas un fichier régulier", path.display()));
    }

    // Vérifier les permissions d'exécution
    let perms = metadata.permissions();
    let is_executable = perms.mode() & 0o111 != 0;
    if !is_executable {
        return Err(format!("{} n'est pas exécutable", path.display()));
    }

    if has_setuid_or_setgid(&metadata) {
        return Err(format!("{} a un bit setuid/setgid", path.display()));
    }

    // Vérifier si déjà compressé
    if is_compressed(path).unwrap_or(false) {
        return Err(format!("{} est déjà compressé", path.display()));
    }

    Ok(metadata)
}

// Générer un script de décompression avec toutes les positions calculées
fn generate_decompression_script(script_start: usize, decomp_size: usize, data_start: usize) -> String {
    format!(r#"#!/bin/sh
# {} - compressed by rust-gzexe
set -e

# Positions calculées par main.rs
SCRIPT_START={}
DECOMP_SIZE={}
DATA_START={}

SCRIPT="$0"

# Créer un répertoire temporaire unique avec PID et timestamp
TMPDIR=/tmp/rust-gzexe.$$.$(date +%s)
mkdir -p "$TMPDIR" || exit 1

# Nettoyage même si le script est tué (INT, TERM, HUP)
trap 'rm -rf "$TMPDIR"' EXIT INT TERM HUP

# Extraire le décompresseur (spécifique à l'algorithme)
dd if="$SCRIPT" bs=1 skip=$SCRIPT_START count=$DECOMP_SIZE of="$TMPDIR/decompress" 2>/dev/null
if [ ! -s "$TMPDIR/decompress" ]; then
    echo "Erreur: extraction du décompresseur échouée" >&2
    exit 1
fi
chmod +x "$TMPDIR/decompress"

# Extraire les données compressées (du DATA_START jusqu'à la fin)
dd if="$SCRIPT" bs=1 skip=$DATA_START of="$TMPDIR/compressed" 2>/dev/null
if [ ! -s "$TMPDIR/compressed" ]; then
    echo "Erreur: extraction des données échouée" >&2
    exit 1
fi

# Décompresser (le décompresseur connaît l'algorithme)
"$TMPDIR/decompress" < "$TMPDIR/compressed" > "$TMPDIR/out"
if [ ! -s "$TMPDIR/out" ]; then
    echo "Erreur: décompression échouée" >&2
    exit 1
fi

chmod +x "$TMPDIR/out"

# Exécuter le programme décompressé
exec "$TMPDIR/out" "$@"
"#, SIGNATURE, script_start, decomp_size, data_start)
}

fn create_compressed_file(
    original_path: &Path,
    algo: &CompressionAlgo,
    metadata: &fs::Metadata,
) -> io::Result<()> {
    println!("🔧 Compression de {}...", original_path.display());
    
    // Lire le fichier original
    let original_data = fs::read(original_path)?;
    let original_size = original_data.len();
    
    // Compresser
    let compressed_data = algo.compress(&original_data)?;
    let compressed_size = compressed_data.len();
    
    // Récupérer le décompresseur spécifique à l'algorithme
    let decompressor = algo.decompressor_bin();
    let decompressor_size = decompressor.len();
    
    // Calculer les positions
    let script_start = SCRIPT_RESERVED_SIZE; // Le décompresseur commence ici
    let data_start = script_start + decompressor_size; // Les données commencent ici
    
    // Calculer les ratios
    let ratio = 100.0 - (compressed_size as f64 / original_size as f64 * 100.0);
    let total_size = compressed_size + decompressor_size;
    let total_ratio = 100.0 - (total_size as f64 / original_size as f64 * 100.0);
    
    println!("  Taille originale: {} octets", original_size);
    println!("  Taille compressée: {} octets", compressed_size);
    println!("  Taille décompresseur: {} octets", decompressor_size);
    println!("  Taille totale (sans script): {} octets", total_size);
    println!("  Ratio compression: {:.1}%", ratio);
    println!("  Ratio final (sans script): {:.1}%", total_ratio);
    println!("  Algorithme: {:?}", algo);
    
    // Générer le script avec les positions
    let script = generate_decompression_script(script_start, decompressor_size, data_start);
    let script_bytes = script.as_bytes();
    let script_size = script_bytes.len();
    
    if script_size > SCRIPT_RESERVED_SIZE {
        return Err(io::Error::new(io::ErrorKind::Other,
            format!("Script trop grand ({} > {} octets)", script_size, SCRIPT_RESERVED_SIZE)));
    }
    
    println!("  Taille du script: {} octets (réservé: {})", script_size, SCRIPT_RESERVED_SIZE);
    println!("  Décompresseur à l'octet: {}", script_start);
    println!("  Données à l'octet: {}", data_start);
    println!("  Taille totale finale: {} octets", SCRIPT_RESERVED_SIZE + decompressor_size + compressed_size);
    
    // Créer un fichier temporaire
    let temp_path = original_path.with_extension("tmp");
    let mut temp_file = fs::File::create(&temp_path)?;
    
    // Écrire le script
    temp_file.write_all(script_bytes)?;
    
    // Compléter avec des zéros jusqu'à SCRIPT_RESERVED_SIZE
    let padding = vec![0u8; SCRIPT_RESERVED_SIZE - script_size];
    temp_file.write_all(&padding)?;
    
    // Écrire le décompresseur (spécifique à l'algorithme)
    temp_file.write_all(decompressor)?;
    
    // Écrire les données compressées
    temp_file.write_all(&compressed_data)?;
    temp_file.sync_all()?;
    
    // Créer un backup de l'original
    let backup_path = original_path.with_extension("orig");
    fs::copy(original_path, &backup_path)?;
    
    // Remplacer l'original par le fichier temporaire
    fs::rename(&temp_path, original_path)?;
    
    // Restaurer les permissions
    fs::set_permissions(original_path, metadata.permissions())?;
    
    println!("✅ Fichier auto-décompressant créé: {}", original_path.display());
    println!("✅ Backup sauvegardé: {}", backup_path.display());
    
    Ok(())
}

fn decompress_file(path: &Path) -> io::Result<()> {
    println!("🔧 Décompression de {}...", path.display());
    
    let data = fs::read(path)?;
    
    if data.len() < SCRIPT_RESERVED_SIZE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, 
            "Fichier trop court"));
    }
    
    // Vérifier la signature dans le script
    let header = String::from_utf8_lossy(&data[0..SCRIPT_RESERVED_SIZE.min(256)]);
    if !header.contains(SIGNATURE) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, 
            "Fichier non compressé (signature introuvable)"));
    }
    
    // Extraire DECOMP_SIZE et DATA_START du script
    // On cherche les lignes "DECOMP_SIZE=..." et "DATA_START=..."
    let content = String::from_utf8_lossy(&data[0..SCRIPT_RESERVED_SIZE]);
    
    let mut decomp_size = 0;
    let mut data_start = 0;
    
    for line in content.lines() {
        if line.starts_with("DECOMP_SIZE=") {
            if let Some(val) = line.split('=').nth(1) {
                decomp_size = val.parse::<usize>().unwrap_or(0);
                println!("  DECOMP_SIZE trouvé: {}", decomp_size);
            }
        }
        if line.starts_with("DATA_START=") {
            if let Some(val) = line.split('=').nth(1) {
                data_start = val.parse::<usize>().unwrap_or(0);
                println!("  DATA_START trouvé: {}", data_start);
            }
        }
    }
    
    if decomp_size == 0 || data_start == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, 
            "Impossible de lire les tailles dans le script"));
    }
    
    // Vérifier que le fichier est assez grand
    if data.len() < data_start {
        return Err(io::Error::new(io::ErrorKind::InvalidData, 
            format!("Fichier trop court: {} < {}", data.len(), data_start)));
    }
    
    // Les données compressées commencent à DATA_START
    let compressed_data = &data[data_start..];
    
    if compressed_data.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, 
            "Données compressées vides"));
    }
    
    // Détecter l'algorithme de compression (pour info)
    let algo = CompressionAlgo::from_magic(compressed_data)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, 
            "Format de compression inconnu"))?;
    
    println!("  Algorithme détecté: {:?}", algo);
    println!("  Taille des données compressées: {} octets", compressed_data.len());
    
    // Décompresser avec l'algorithme détecté
    let decompressed_data = algo.decompress(compressed_data)?;
    
    println!("  Taille décompressée: {} octets", decompressed_data.len());
    
    // Sauvegarder le fichier compressé original
    let compressed_backup = path.with_extension("compressed");
    fs::rename(path, &compressed_backup)?;
    
    // Écrire le fichier décompressé
    fs::write(path, &decompressed_data)?;
    
    // Restaurer les permissions depuis le backup
    if let Ok(meta) = fs::metadata(&compressed_backup) {
        fs::set_permissions(path, meta.permissions())?;
    }
    
    println!("✅ Fichier décompressé: {}", path.display());
    println!("✅ Backup du compressé: {}", compressed_backup.display());
    
    Ok(())
}

fn main() {
    let config = match Config::parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("❌ Erreur: {}", e);
            process::exit(1);
        }
    };

    let mut exit_code = 0;

    for path in &config.files {
        if config.decompress {
            match decompress_file(path) {
                Ok(()) => println!("✅ Succès\n"),
                Err(e) => {
                    eprintln!("❌ Erreur décompression {}: {}\n", path.display(), e);
                    exit_code = 1;
                }
            }
        } else {
            let metadata = match check_file(path) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("❌ Erreur: {}\n", e);
                    exit_code = 1;
                    continue;
                }
            };

            let algo = config.algo.as_ref().unwrap();
            match create_compressed_file(path, algo, &metadata) {
                Ok(()) => println!("✅ Succès\n"),
                Err(e) => {
                    eprintln!("❌ Erreur compression {}: {}\n", path.display(), e);
                    exit_code = 1;
                }
            }
        }
    }

    process::exit(exit_code);
}

