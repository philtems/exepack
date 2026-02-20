use std::env;
use std::fs;
use std::io::{self, Write, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process;

const MAGIC: &str = "# compressed by tems-exepack\n";
const VERSION: &str = "0.5.0";
const AUTHOR: &str = "Philippe TEMESI";
const WEBSITE: &str = "https://www.tems.be";
const YEAR: &str = "2026";

// Constantes pour la méthode dd
const SCRIPT_RESERVED_SIZE: usize = 4096; // Espace réservé pour le script (4K)
const SIGNATURE: &str = "TEMS-EXEPACK:v1";

#[derive(Debug, Clone, Copy)]
enum CompressionAlgo {
    Gzip,
    Bzip2,
    Xz,
    TemsXz,  // Même format XZ mais avec décompresseur embarqué
}

impl CompressionAlgo {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "-gz" => Some(CompressionAlgo::Gzip),
            "-bz2" => Some(CompressionAlgo::Bzip2),
            "-xz" => Some(CompressionAlgo::Xz),
            "-temsxz" => Some(CompressionAlgo::TemsXz),
            _ => None,
        }
    }

    fn to_str(&self) -> &'static str {
        match self {
            CompressionAlgo::Gzip => "gzip",
            CompressionAlgo::Bzip2 => "bzip2",
            CompressionAlgo::Xz => "xz",
            CompressionAlgo::TemsXz => "temsxz (embedded)",
        }
    }

    fn decompressor_bin(&self) -> Option<&'static [u8]> {
        match self {
            CompressionAlgo::TemsXz => {
                Some(include_bytes!(concat!(env!("OUT_DIR"), "/decompressors/decompress_temsxz.bin")))
            }
            _ => None,
        }
    }

    // Compression avec les bibliothèques Rust (pour tous les algorithmes)
    fn compress_with_rust(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        match self {
            CompressionAlgo::Gzip => {
                use flate2::write::GzEncoder;
                use flate2::Compression;
                let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
                encoder.write_all(data)?;
                encoder.finish()
            }
            CompressionAlgo::Bzip2 => {
                use bzip2::write::BzEncoder;
                use bzip2::Compression;
                let mut encoder = BzEncoder::new(Vec::new(), Compression::best());
                encoder.write_all(data)?;
                encoder.finish()
            }
            CompressionAlgo::Xz | CompressionAlgo::TemsXz => {
                use xz2::write::XzEncoder;
                // Niveau 9 + flags extreme pour la compression maximale
                // xz2 ne supporte pas directement LZMA_PRESET_EXTREME, mais niveau 9 est déjà très agressif
                let mut encoder = XzEncoder::new(Vec::new(), 9);
                encoder.write_all(data)?;
                encoder.finish()
            }
        }
    }

    fn compress(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        self.compress_with_rust(data)
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
            CompressionAlgo::Bzip2 => {
                use bzip2::read::BzDecoder;
                let mut decoder = BzDecoder::new(data);
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed)?;
                Ok(decompressed)
            }
            CompressionAlgo::Xz => {
                use xz2::read::XzDecoder;
                let mut decoder = XzDecoder::new(data);
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed)?;
                Ok(decompressed)
            }
            CompressionAlgo::TemsXz => {
                if let Some(decomp_bin) = self.decompressor_bin() {
                    let pid = std::process::id();
                    let temp_dir = format!("/tmp/tems-exepack-decomp-{}", pid);
                    fs::create_dir_all(&temp_dir)?;
                    
                    let decomp_path = format!("{}/decomp", temp_dir);
                    let input_path = format!("{}/input", temp_dir);
                    let output_path = format!("{}/output", temp_dir);
                    
                    fs::write(&decomp_path, decomp_bin)?;
                    fs::set_permissions(&decomp_path, fs::Permissions::from_mode(0o755))?;
                    fs::write(&input_path, data)?;
                    
                    // temsxz without parameters decompresses
                    let status = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(format!("cat {} | {} > {}", input_path, decomp_path, output_path))
                        .status()?;
                    
                    if !status.success() {
                        let _ = fs::remove_dir_all(&temp_dir);
                        return Err(io::Error::new(io::ErrorKind::Other, "TEMS XZ decompression failed"));
                    }
                    
                    let decompressed = fs::read(&output_path)?;
                    let _ = fs::remove_dir_all(&temp_dir);
                    Ok(decompressed)
                } else {
                    Err(io::Error::new(io::ErrorKind::NotFound, "TEMS XZ decompressor binary not found"))
                }
            }
        }
    }

    fn from_magic(data: &[u8]) -> Option<Self> {
        if data.starts_with(&[0x1f, 0x8b]) {
            Some(CompressionAlgo::Gzip)
        } else if data.starts_with(b"BZh") {
            Some(CompressionAlgo::Bzip2)
        } else if data.starts_with(&[0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00]) {
            Some(CompressionAlgo::Xz)  // Note: TemsXz utilise le même magic que Xz
        } else {
            None
        }
    }
}

// Générer un script de décompression avec dd pour OpenBSD/systèmes sans /proc
fn generate_dd_decompression_script(script_start: usize, decomp_size: usize, data_start: usize, algo: CompressionAlgo) -> String {
    let algo_name = match algo {
        CompressionAlgo::Gzip => "gzip",
        CompressionAlgo::Bzip2 => "bzip2",
        CompressionAlgo::Xz => "xz",
        CompressionAlgo::TemsXz => "temsxz",
    };
    
    let decompressor_cmd = match algo {
        CompressionAlgo::TemsXz => "\"$TMPDIR/decompress\"",
        _ => algo_name,
    };
    
    format!(r#"#!/bin/sh
# {} - compressed by tems-exepack (dd method)
# (c) {} {}, {}
set -e

# Positions calculées
SCRIPT_START={}
DECOMP_SIZE={}
DATA_START={}

SCRIPT="$0"

# Créer un répertoire temporaire unique
TMPDIR=/tmp/tems-exepack.$$.$(date +%s)
mkdir -p "$TMPDIR" || exit 1

# Nettoyage
trap 'rm -rf "$TMPDIR"' EXIT INT TERM HUP

# Extraire le décompresseur (si nécessaire)
if [ $DECOMP_SIZE -gt 0 ]; then
    dd if="$SCRIPT" bs=1 skip=$SCRIPT_START count=$DECOMP_SIZE of="$TMPDIR/decompress" 2>/dev/null
    if [ ! -s "$TMPDIR/decompress" ]; then
        echo "Error: decompressor extraction failed" >&2
        exit 1
    fi
    chmod +x "$TMPDIR/decompress"
    DECOMP_CMD="$TMPDIR/decompress"
else
    # Utiliser le compresseur système
    DECOMP_CMD="{}"
fi

# Extraire les données compressées
dd if="$SCRIPT" bs=1 skip=$DATA_START of="$TMPDIR/compressed" 2>/dev/null
if [ ! -s "$TMPDIR/compressed" ]; then
    echo "Error: data extraction failed" >&2
    exit 1
fi

# Décompresser
if [ "$DECOMP_CMD" = "gzip" ] || [ "$DECOMP_CMD" = "bzip2" ] || [ "$DECOMP_CMD" = "xz" ]; then
    # Compresseur système
    $DECOMP_CMD -d -c < "$TMPDIR/compressed" > "$TMPDIR/out"
else
    # Décompresseur intégré
    cat "$TMPDIR/compressed" | $DECOMP_CMD > "$TMPDIR/out"
fi

if [ ! -s "$TMPDIR/out" ]; then
    echo "Error: decompression failed" >&2
    exit 1
fi

chmod +x "$TMPDIR/out"
exec "$TMPDIR/out" "$@"
"#, SIGNATURE, AUTHOR, WEBSITE, YEAR, script_start, decomp_size, data_start, decompressor_cmd)
}

// Version modifiée de create_compressed_file avec support dd
fn create_compressed_file_dd(
    original_path: &Path,
    algo: &CompressionAlgo,
    metadata: &fs::Metadata,
    keep_original: bool,
    use_dd_method: bool, // true pour utiliser dd, false pour méthode awk
) -> io::Result<()> {
    println!("🔧 Compressing {} with {:?}...", original_path.display(), algo);
    
    // Lire le fichier original
    let original_data = fs::read(original_path)?;
    let original_size = original_data.len();
    
    // Compresser avec le niveau maximum
    let start = std::time::Instant::now();
    let compressed_data = algo.compress(&original_data)?;
    let duration = start.elapsed();
    
    let compressed_size = compressed_data.len();
    
    // Taille du décompresseur (seulement pour TEMS XZ)
    let decompressor_size = if matches!(algo, CompressionAlgo::TemsXz) {
        if let Some(decomp_bin) = algo.decompressor_bin() {
            decomp_bin.len()
        } else {
            0
        }
    } else {
        0
    };
    
    let ratio = 100.0 - (compressed_size as f64 / original_size as f64 * 100.0);
    let total_size = compressed_size + decompressor_size;
    let total_ratio = 100.0 - (total_size as f64 / original_size as f64 * 100.0);
    
    println!("  Compression time: {:.2?}", duration);
    println!("  Original size: {} bytes", original_size);
    println!("  Compressed size: {} bytes", compressed_size);
    println!("  Compression ratio: {:.1}%", ratio);
    
    if decompressor_size > 0 {
        println!("  Decompressor size: {} bytes", decompressor_size);
        println!("  Total size with decompressor: {} bytes", total_size);
        println!("  Final ratio with decompressor: {:.1}%", total_ratio);
    }
    
    // Créer un fichier temporaire
    let temp_path = original_path.with_extension("tmp");
    let mut temp_file = fs::File::create(&temp_path)?;
    
    if use_dd_method {
        // MÉTHODE DD: positions fixes
        let script_start = SCRIPT_RESERVED_SIZE; // Le décompresseur commence ici
        let data_start = script_start + decompressor_size; // Les données commencent ici
        
        // Générer le script dd avec les positions
        let script = generate_dd_decompression_script(script_start, decompressor_size, data_start, *algo);
        let script_bytes = script.as_bytes();
        let script_size = script_bytes.len();
        
        if script_size > SCRIPT_RESERVED_SIZE {
            return Err(io::Error::new(io::ErrorKind::Other,
                format!("Script too large ({} > {} bytes)", script_size, SCRIPT_RESERVED_SIZE)));
        }
        
        println!("  Script size: {} bytes (reserved: {})", script_size, SCRIPT_RESERVED_SIZE);
        println!("  Decompressor at byte: {}", script_start);
        println!("  Data at byte: {}", data_start);
        
        // Écrire le script
        temp_file.write_all(script_bytes)?;
        
        // Compléter avec des zéros jusqu'à SCRIPT_RESERVED_SIZE
        let padding = vec![0u8; SCRIPT_RESERVED_SIZE - script_size];
        temp_file.write_all(&padding)?;
        
        // Écrire le décompresseur (pour TEMS XZ)
        if decompressor_size > 0 {
            if let Some(decomp_bin) = algo.decompressor_bin() {
                temp_file.write_all(decomp_bin)?;
            }
        }
        
        // Écrire les données compressées
        temp_file.write_all(&compressed_data)?;
        
    } else {
        // MÉTHODE AWK (méthode standard)
        let script = generate_awk_decompression_script(*algo);
        temp_file.write_all(script.as_bytes())?;
        
        // Pour TEMS XZ seulement, inclure le décompresseur
        if decompressor_size > 0 {
            if let Some(decomp_bin) = algo.decompressor_bin() {
                temp_file.write_all(decomp_bin)?;
            }
            temp_file.write_all(b"\n")?;
        }
        
        temp_file.write_all(b"__DATA__\n")?;
        temp_file.write_all(&compressed_data)?;
    }
    
    temp_file.sync_all()?;
    
    // Sauvegarder l'original si demandé
    if keep_original {
        let backup_path = original_path.with_extension("orig");
        fs::copy(original_path, &backup_path)?;
        println!("✅ Backup saved: {}", backup_path.display());
    }
    
    // Remplacer par le fichier compressé
    fs::rename(&temp_path, original_path)?;
    fs::set_permissions(original_path, metadata.permissions())?;
    
    println!("✅ Self-decompressing file created: {}", original_path.display());
    
    Ok(())
}

// Générer un script de décompression avec awk (méthode standard)
fn generate_awk_decompression_script(algo: CompressionAlgo) -> String {
    let header = format!(
        "#!/bin/sh\n\
         # compressed by tems-exepack\n\
         # (c) {} {}, {}\n\
         set -e\n\n\
         # Create temporary directory\n\
         TMPDIR=/tmp/tems-exepack.$$\n\
         mkdir -p \"$TMPDIR\" || exit 1\n\
         trap 'rm -rf \"$TMPDIR\"' EXIT\n\n\
         # Find markers\n\
         SCRIPT=\"$0\"\n\
         DECOMP_LINENUM=$(awk '/^__DECOMPRESSOR__$/ {{print NR; exit}}' \"$SCRIPT\")\n\
         DATA_LINENUM=$(awk '/^__DATA__$/ {{print NR; exit}}' \"$SCRIPT\")\n\n\
         if [ -z \"$DECOMP_LINENUM\" ] || [ -z \"$DATA_LINENUM\" ]; then\n\
             echo \"Invalid format: missing markers\" >&2\n\
             exit 1\n\
         fi\n\n",
        AUTHOR, WEBSITE, YEAR
    );

    let decompressor_part = match algo {
        CompressionAlgo::Gzip => {
            String::from(
                "# Use gzip for decompression\n\
                 # Extract compressed data (after __DATA__)\n\
                 tail -n +$((DATA_LINENUM + 1)) \"$SCRIPT\" > \"$TMPDIR/compressed\"\n\n\
                 # Verify extraction\n\
                 if [ ! -s \"$TMPDIR/compressed\" ]; then\n\
                     echo \"Error: data extraction failed\" >&2\n\
                     exit 1\n\
                 fi\n\n\
                 # Decompress with gzip\n\
                 gzip -d -c < \"$TMPDIR/compressed\" > \"$TMPDIR/out\"\n"
            )
        }
        CompressionAlgo::Bzip2 => {
            String::from(
                "# Use bzip2 for decompression\n\
                 # Extract compressed data (after __DATA__)\n\
                 tail -n +$((DATA_LINENUM + 1)) \"$SCRIPT\" > \"$TMPDIR/compressed\"\n\n\
                 # Verify extraction\n\
                 if [ ! -s \"$TMPDIR/compressed\" ]; then\n\
                     echo \"Error: data extraction failed\" >&2\n\
                     exit 1\n\
                 fi\n\n\
                 # Decompress with bzip2\n\
                 bzip2 -d -c < \"$TMPDIR/compressed\" > \"$TMPDIR/out\"\n"
            )
        }
        CompressionAlgo::Xz => {
            String::from(
                "# Use xz for decompression\n\
                 # Extract compressed data (after __DATA__)\n\
                 tail -n +$((DATA_LINENUM + 1)) \"$SCRIPT\" > \"$TMPDIR/compressed\"\n\n\
                 # Verify extraction\n\
                 if [ ! -s \"$TMPDIR/compressed\" ]; then\n\
                     echo \"Error: data extraction failed\" >&2\n\
                     exit 1\n\
                 fi\n\n\
                 # Decompress with xz\n\
                 xz -d -c < \"$TMPDIR/compressed\" > \"$TMPDIR/out\"\n"
            )
        }
        CompressionAlgo::TemsXz => {
            String::from(
                "# Extract TEMS XZ decompressor (between __DECOMPRESSOR__ and __DATA__)\n\
                 tail -n +$((DECOMP_LINENUM + 1)) \"$SCRIPT\" | head -n $((DATA_LINENUM - DECOMP_LINENUM - 1)) > \"$TMPDIR/decompress\"\n\
                 chmod +x \"$TMPDIR/decompress\"\n\n\
                 # Extract compressed data (after __DATA__)\n\
                 tail -n +$((DATA_LINENUM + 1)) \"$SCRIPT\" > \"$TMPDIR/compressed\"\n\n\
                 # Verify extraction\n\
                 if [ ! -s \"$TMPDIR/decompress\" ] || [ ! -s \"$TMPDIR/compressed\" ]; then\n\
                     echo \"Error: data extraction failed\" >&2\n\
                     exit 1\n\
                 fi\n\n\
                 # Decompress with embedded decompressor (temsxz without parameters)\n\
                 cat \"$TMPDIR/compressed\" | \"$TMPDIR/decompress\" > \"$TMPDIR/out\"\n"
            )
        }
    };

    format!(
        "{}\
         {}\n\n\
         # Verify decompression success\n\
         if [ ! -s \"$TMPDIR/out\" ]; then\n\
             echo \"Error: decompression failed\" >&2\n\
             exit 1\n\
         fi\n\n\
         chmod +x \"$TMPDIR/out\"\n\
         exec \"$TMPDIR/out\" \"$@\"\n\
         __DECOMPRESSOR__\n",
        header, decompressor_part
    )
}

struct Config {
    algo: Option<CompressionAlgo>,
    decompress: bool,
    files: Vec<PathBuf>,
    keep: bool,
    dd_method: bool, // Utiliser la méthode dd
}

impl Config {
    fn parse_args() -> Result<Self, String> {
        let args: Vec<String> = env::args().collect();
        
        if args.len() < 2 {
            return Err(format!(
                "tems-exepack version {} (c) {} {}\n\
                 Usage: tems-exepack [OPTIONS] <files...>\n\n\
                 OPTIONS:\n\
                 \x20 -gz        Gzip compression (Rust) - decompression with system gzip\n\
                 \x20 -bz2       Bzip2 compression (Rust) - decompression with system bzip2\n\
                 \x20 -xz        XZ compression (Rust) - decompression with system xz\n\
                 \x20 -temsxz    XZ compression (Rust, max) - decompression with embedded temsxz\n\
                 \x20 -d         Decompress files\n\
                 \x20 -k         Keep original files (backup)\n\
                 \x20 -dd        Use dd method for compatibility (OpenBSD, etc.)\n\
                 \x20 -h         Show this help",
                VERSION, AUTHOR, WEBSITE
            ));
        }

        let mut config = Config {
            algo: None,
            decompress: false,
            files: Vec::new(),
            keep: false,
            dd_method: false,
        };

        if args.len() == 2 && (args[1] == "-h" || args[1] == "--help") {
            return Err(format!(
                "tems-exepack version {} (c) {} {}\n\n\
                 DESCRIPTION:\n\
                 Compress executable files to create self-decompressing binaries.\n\n\
                 USAGE:\n\
                 \x20 tems-exepack [OPTIONS] <files...>\n\n\
                 COMPRESSION OPTIONS:\n\
                 \x20 -gz        Gzip compression (Rust library) - decompression with system gzip\n\
                 \x20 -bz2       Bzip2 compression (Rust library) - decompression with system bzip2\n\
                 \x20 -xz        XZ compression (Rust library) - decompression with system xz\n\
                 \x20 -temsxz    XZ compression (Rust, maximum) - decompression with embedded temsxz\n\n\
                 OTHER OPTIONS:\n\
                 \x20 -d         Decompress files\n\
                 \x20 -k         Keep original files (backup)\n\
                 \x20 -dd        Use dd method for compatibility (OpenBSD, systems without /proc)\n\
                 \x20 -h         Show this help\n\n\
                 EXAMPLES:\n\
                 \x20 tems-exepack -xz /usr/local/bin/program\n\
                 \x20 tems-exepack -d program\n\
                 \x20 tems-exepack -temsxz -dd -k my_program\n\n\
                 WEBSITE: {}\n\
                 YEAR: {}",
                VERSION, AUTHOR, WEBSITE, WEBSITE, YEAR
            ));
        }

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "-gz" | "-bz2" | "-xz" | "-temsxz" => {
                    if config.algo.is_some() {
                        return Err("Only one compression algorithm can be specified".to_string());
                    }
                    config.algo = CompressionAlgo::from_str(&args[i]);
                }
                "-d" => {
                    config.decompress = true;
                }
                "-k" => {
                    config.keep = true;
                }
                "-dd" => {
                    config.dd_method = true;
                }
                arg if arg.starts_with('-') && arg != "-h" && arg != "--help" => {
                    return Err(format!("Unknown option: {}", arg));
                }
                _ => {
                    config.files.push(PathBuf::from(&args[i]));
                }
            }
            i += 1;
        }

        if config.files.is_empty() {
            return Err("No files specified".to_string());
        }

        if !config.decompress && config.algo.is_none() {
            config.algo = Some(CompressionAlgo::Gzip); // Default to gzip
        }

        Ok(config)
    }
}

fn is_compressed(path: &Path) -> io::Result<bool> {
    let data = fs::read(path)?;
    let magic_bytes = MAGIC.as_bytes();
    Ok(data.windows(magic_bytes.len())
        .any(|window| window == magic_bytes))
}

fn has_setuid_or_setgid(metadata: &fs::Metadata) -> bool {
    #[cfg(target_os = "linux")]
    {
        use std::os::linux::fs::MetadataExt;
        let mode = metadata.st_mode();
        (mode & 0o4000) != 0 || (mode & 0o2000) != 0
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mode = metadata.mode();
        (mode & 0o4000) != 0 || (mode & 0o2000) != 0
    }
}

fn check_file(path: &Path) -> Result<fs::Metadata, String> {
    let metadata = fs::metadata(path)
        .map_err(|e| format!("Error reading {}: {}", path.display(), e))?;

    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }

    let perms = metadata.permissions();
    let is_executable = perms.mode() & 0o111 != 0;
    if !is_executable {
        return Err(format!("{} is not executable", path.display()));
    }

    if has_setuid_or_setgid(&metadata) {
        return Err(format!("{} has setuid/setgid bit", path.display()));
    }

    if is_compressed(path).unwrap_or(false) {
        return Err(format!("{} is already compressed", path.display()));
    }

    Ok(metadata)
}

fn decompress_file(path: &Path, keep_compressed: bool) -> io::Result<()> {
    println!("🔧 Decompressing {}...", path.display());
    
    let data = fs::read(path)?;
    
    // Chercher le magic dans le fichier
    let magic_bytes = MAGIC.as_bytes();
    let magic_pos = data.windows(magic_bytes.len())
        .position(|window| window == magic_bytes);
    
    let magic_pos = match magic_pos {
        Some(pos) => pos,
        None => return Err(io::Error::new(io::ErrorKind::InvalidData, 
            "File not compressed (magic not found)"))
    };
    
    // Chercher __DATA__
    let search_start = magic_pos;
    let data_start = data[search_start..].windows(9)
        .position(|w| w == b"__DATA__\n")
        .map(|pos| search_start + pos + 9)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, 
            "Invalid format: missing __DATA__ marker"))?;
    
    let compressed_data = &data[data_start..];
    
    if compressed_data.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, 
            "Empty compressed data"));
    }
    
    // Détecter l'algorithme de compression
    let algo = CompressionAlgo::from_magic(compressed_data)
        .unwrap_or(CompressionAlgo::Gzip); // Fallback to gzip if unknown
    
    println!("  Detected algorithm: {:?}", algo);
    println!("  Compressed data size: {} bytes", compressed_data.len());
    
    // Décompresser
    let start = std::time::Instant::now();
    let decompressed_data = algo.decompress(compressed_data)?;
    let duration = start.elapsed();
    
    println!("  Decompression time: {:.2?}", duration);
    println!("  Decompressed size: {} bytes", decompressed_data.len());
    
    // Sauvegarder le fichier compressé original si demandé
    if keep_compressed {
        let compressed_backup = path.with_extension("compressed");
        fs::rename(path, &compressed_backup)?;
        println!("✅ Compressed backup saved: {}", compressed_backup.display());
    } else {
        fs::remove_file(path)?;
    }
    
    // Écrire le fichier décompressé
    fs::write(path, &decompressed_data)?;
    
    // Restaurer les permissions
    if let Ok(meta) = fs::metadata(path) {
        fs::set_permissions(path, meta.permissions())?;
    }
    
    println!("✅ Decompressed file: {}", path.display());
    
    Ok(())
}

fn main() {
    let config = match Config::parse_args() {
        Ok(c) => c,
        Err(e) => {
            println!("{}", e);
            process::exit(1);
        }
    };

    let mut exit_code = 0;

    for path in &config.files {
        if config.decompress {
            match decompress_file(path, config.keep) {
                Ok(()) => println!("✅ Success\n"),
                Err(e) => {
                    eprintln!("❌ Error decompressing {}: {}\n", path.display(), e);
                    exit_code = 1;
                }
            }
        } else {
            let metadata = match check_file(path) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("❌ Error: {}\n", e);
                    exit_code = 1;
                    continue;
                }
            };

            let algo = config.algo.as_ref().unwrap();
            match create_compressed_file_dd(path, algo, &metadata, config.keep, config.dd_method) {
                Ok(()) => println!("✅ Success\n"),
                Err(e) => {
                    eprintln!("❌ Error compressing {}: {}\n", path.display(), e);
                    exit_code = 1;
                }
            }
        }
    }

    process::exit(exit_code);
}

