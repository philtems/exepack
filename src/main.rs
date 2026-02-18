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
            return Err("Usage: tems-exepack [-gz | -zstd | -xz] [-d] <fichiers...>".to_string());
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

// Vérifier si le fichier est déjà compressé
fn is_compressed(path: &Path) -> io::Result<bool> {
    let data = fs::read(path)?;
    let magic_bytes = MAGIC.as_bytes();
    Ok(data.windows(magic_bytes.len())
        .any(|window| window == magic_bytes))
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

// Générer un script de décompression avec décompresseur intégré
fn generate_decompression_script() -> String {
    r#"#!/bin/sh
# compressed by rust-gzexe
set -e

# Créer un répertoire temporaire
TMPDIR=/tmp/rust-gzexe.$$
mkdir -p "$TMPDIR" || exit 1
trap 'rm -rf "$TMPDIR"' EXIT

# Trouver les marqueurs
SCRIPT="$0"
DECOMP_LINENUM=$(awk '/^__DECOMPRESSOR__$/ {print NR; exit}' "$SCRIPT")
DATA_LINENUM=$(awk '/^__DATA__$/ {print NR; exit}' "$SCRIPT")

if [ -z "$DECOMP_LINENUM" ] || [ -z "$DATA_LINENUM" ]; then
    echo "Format invalide: marqueurs manquants" >&2
    exit 1
fi

# Extraire le décompresseur (entre __DECOMPRESSOR__ et __DATA__)
tail -n +$((DECOMP_LINENUM + 1)) "$SCRIPT" | head -n $((DATA_LINENUM - DECOMP_LINENUM - 1)) > "$TMPDIR/decompress"
chmod +x "$TMPDIR/decompress"

# Extraire les données compressées (après __DATA__)
tail -n +$((DATA_LINENUM + 1)) "$SCRIPT" > "$TMPDIR/compressed"

# Vérifier que les fichiers ont été extraits
if [ ! -s "$TMPDIR/decompress" ] || [ ! -s "$TMPDIR/compressed" ]; then
    echo "Erreur: extraction des données échouée" >&2
    exit 1
fi

# Décompresser
"$TMPDIR/decompress" < "$TMPDIR/compressed" > "$TMPDIR/out"

# Vérifier que la décompression a réussi
if [ ! -s "$TMPDIR/out" ]; then
    echo "Erreur: décompression échouée" >&2
    exit 1
fi

chmod +x "$TMPDIR/out"
exec "$TMPDIR/out" "$@"
__DECOMPRESSOR__
__DATA__
"#.to_string()
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
    
    // Récupérer le décompresseur spécifique
    let decompressor = algo.decompressor_bin();
    let decompressor_size = decompressor.len();
    
    // Calculer le ratio
    let ratio = 100.0 - (compressed_size as f64 / original_size as f64 * 100.0);
    let total_size = compressed_size + decompressor_size;
    let total_ratio = 100.0 - (total_size as f64 / original_size as f64 * 100.0);
    
    println!("  Taille originale: {} octets", original_size);
    println!("  Taille compressée: {} octets", compressed_size);
    println!("  Taille décompresseur: {} octets", decompressor_size);
    println!("  Taille totale: {} octets", total_size);
    println!("  Ratio compression: {:.1}%", ratio);
    println!("  Ratio final (avec décompresseur): {:.1}%", total_ratio);
    println!("  Algorithme: {:?}", algo);
    
    // Créer un fichier temporaire
    let temp_path = original_path.with_extension("tmp");
    let mut temp_file = fs::File::create(&temp_path)?;
    
    // Écrire le script de décompression
    let script = generate_decompression_script();
    temp_file.write_all(script.as_bytes())?;
    
    // Écrire le décompresseur
    temp_file.write_all(decompressor)?;
    temp_file.write_all(b"\n")?;
    
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
    
    // Chercher le magic dans le fichier
    let magic_bytes = MAGIC.as_bytes();
    let magic_pos = data.windows(magic_bytes.len())
        .position(|window| window == magic_bytes);
    
    let magic_pos = match magic_pos {
        Some(pos) => pos,
        None => return Err(io::Error::new(io::ErrorKind::InvalidData, 
            "Fichier non compressé (magic introuvable)"))
    };
    
    // Chercher __DATA__
    let search_start = magic_pos;
    let data_start = data[search_start..].windows(9)
        .position(|w| w == b"__DATA__\n")
        .map(|pos| search_start + pos + 9)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, 
            "Format invalide: pas de marqueur __DATA__"))?;
    
    let compressed_data = &data[data_start..];
    
    if compressed_data.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, 
            "Données compressées vides"));
    }
    
    // Détecter l'algorithme de compression
    let algo = CompressionAlgo::from_magic(compressed_data)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, 
            "Format de compression inconnu"))?;
    
    println!("  Algorithme détecté: {:?}", algo);
    println!("  Taille des données compressées: {} octets", compressed_data.len());
    
    // Décompresser
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

