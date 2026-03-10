use std::env;
use std::fs;
use std::io::{self, Write, Read};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process;
use std::num::NonZeroU64;

use zopfli::{GzipEncoder, Options, BlockType};
use flate2::read::GzDecoder;

const MAGIC: &[u8] = b"# compressed by zexe";
const HEADER_SIZE: usize = 512;
const AUTHOR: &str = "Philippe TEMESI";
const YEAR: &str = "2026";
const WEBSITE: &str = "https://www.tems.be";

#[derive(Debug)]
struct Config {
    decompress: bool,
    files: Vec<PathBuf>,
}

#[derive(Debug)]
struct FileInfo {
    path: PathBuf,
    original_size: u64,
    compressed_size: u64,
}

impl FileInfo {
    fn compression_ratio(&self) -> f64 {
        if self.original_size == 0 {
            0.0
        } else {
            (self.original_size - self.compressed_size) as f64 * 100.0 / self.original_size as f64
        }
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let config = parse_args()?;
    let mut exit_code = 0;

    for file in config.files {
        let result = if config.decompress {
            decompress_file(&file)
        } else {
            compress_file(&file)
        };

        match result {
            Ok(Some(info)) => {
                if config.decompress {
                    println!("{}: decompressed ({} -> {} bytes, {:.1}% saved)",
                             info.path.display(), info.compressed_size, info.original_size,
                             info.compression_ratio());
                } else {
                    println!("{}: {} -> {} bytes, {:.1}% compression (Zopfli)",
                             info.path.display(), info.original_size, info.compressed_size,
                             info.compression_ratio());
                }
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("{}: {}", file.display(), e);
                exit_code = 1;
            }
        }
    }

    process::exit(exit_code);
}

fn parse_args() -> io::Result<Config> {
    let args: Vec<String> = env::args().collect();
    let mut decompress = false;
    let mut files = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-d" => decompress = true,
            "-h" | "--help" => {
                println!("zexe - Self-extracting executable compressor");
                println!("Author: {} ({}) {}", AUTHOR, YEAR, WEBSITE);
                println!();
                println!("Usage: {} [-d] file...", args[0]);
                println!("  -d    Decompress the file");
                println!();
                println!("Compression: Zopfli (maximum compression, slower but better than gzip)");
                process::exit(0);
            }
            "-V" | "--version" => {
                println!("zexe version 0.1.0 (Zopfli)");
                println!("Author: {} ({}) {}", AUTHOR, YEAR, WEBSITE);
                process::exit(0);
            }
            arg if arg.starts_with('-') => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                    format!("Unknown option: {}", arg)));
            }
            _ => files.push(PathBuf::from(&args[i])),
        }
        i += 1;
    }

    if files.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "No files specified"));
    }

    Ok(Config { decompress, files })
}

fn is_compressed(path: &Path) -> io::Result<bool> {
    let mut file = fs::File::open(path)?;
    let mut magic = [0u8; MAGIC.len()];
    
    // Skip first line
    let mut byte = [0u8; 1];
    while file.read(&mut byte)? == 1 && byte[0] != b'\n' {}
    
    // Read magic
    if file.read(&mut magic)? != MAGIC.len() {
        return Ok(false);
    }
    
    Ok(magic == MAGIC)
}

fn check_file(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound,
            "file does not exist"));
    }

    if !path.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "not a regular file"));
    }

    let metadata = fs::metadata(path)?;
    let permissions = metadata.permissions();
    
    if permissions.mode() & 0o111 == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "not executable"));
    }

    if metadata.mode() & 0o6000 != 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "has setuid/setgid bits set"));
    }

    Ok(())
}

fn compress_file(path: &Path) -> io::Result<Option<FileInfo>> {
    if is_compressed(path)? {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists,
            "file already compressed"));
    }

    check_file(path)?;

    // Create backup
    let backup = path.with_extension("~");
    fs::copy(path, &backup)?;

    // Read original
    let original_data = fs::read(path)?;
    let original_size = original_data.len() as u64;

    // Compress with Zopfli (maximum compression)
    println!("Compressing {} with Zopfli (this may take a while)...", path.display());
    let compressed = compress_zopfli(&original_data)?;
    let compressed_size = compressed.len() as u64;

    // Generate header with fixed size
    let header = format!(
        r#"#!/bin/sh
# compressed by zexe (Zopfli)
# This script is exactly {} bytes long
tmp=`mktemp -d /tmp/zexe.XXXXXXXXXX` || exit 1
trap 'rm -rf "$tmp"' 0
tail -c +{} "$0" | gzip -dc > "$tmp/prog" 2>/dev/null && \
    chmod u+x "$tmp/prog" && exec "$tmp/prog" "$@"
exit $?
"#,
        HEADER_SIZE, HEADER_SIZE + 1
    );
    
    // Pad header to exactly HEADER_SIZE bytes
    let mut header_bytes = header.into_bytes();
    header_bytes.resize(HEADER_SIZE, b'#');
    header_bytes[HEADER_SIZE - 1] = b'\n';

    // Create compressed file with header
    let temp_path = path.with_extension(".tmp");
    let mut final_file = fs::File::create(&temp_path)?;
    final_file.write_all(&header_bytes)?;
    final_file.write_all(&compressed)?;
    final_file.sync_all()?;

    // Copy permissions
    let metadata = fs::metadata(path)?;
    fs::set_permissions(&temp_path, metadata.permissions())?;

    // Replace original
    fs::rename(&temp_path, path)?;

    Ok(Some(FileInfo {
        path: path.to_path_buf(),
        original_size,
        compressed_size: compressed_size + header_bytes.len() as u64,
    }))
}

fn decompress_file(path: &Path) -> io::Result<Option<FileInfo>> {
    if !is_compressed(path)? {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "file not compressed"));
    }

    let data = fs::read(path)?;
    let compressed_size = data.len() as u64;

    if data.len() <= HEADER_SIZE {
        return Err(io::Error::new(io::ErrorKind::InvalidData,
            "corrupted compressed file"));
    }

    // Decompress from HEADER_SIZE (using flate2 for decompression)
    let mut decoder = GzDecoder::new(&data[HEADER_SIZE..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    let original_size = decompressed.len() as u64;

    // Save
    let temp_path = path.with_extension(".tmp");
    fs::write(&temp_path, &decompressed)?;

    let metadata = fs::metadata(path)?;
    fs::set_permissions(&temp_path, metadata.permissions())?;

    fs::rename(&temp_path, path)?;

    Ok(Some(FileInfo {
        path: path.to_path_buf(),
        original_size,
        compressed_size,
    }))
}

fn compress_zopfli(data: &[u8]) -> io::Result<Vec<u8>> {
    // Configuration Zopfli pour une compression maximale
    // Tous les champs doivent être NonZeroU64
    let options = Options {
        iteration_count: NonZeroU64::new(15).unwrap(),  // 15 itérations
        iterations_without_improvement: NonZeroU64::new(3).unwrap(), // Arrête après 3 itérations sans amélioration
        maximum_block_splits: 15, // u16, pas besoin de NonZero
    };
    
    // Type de bloc : Dynamic donne la meilleure compression
    let block_type = BlockType::Dynamic;
    
    let mut compressed = Vec::new();
    
    // Créer l'encodeur
    let mut encoder = GzipEncoder::new(options, block_type, &mut compressed)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Zopfli init error: {}", e)))?;
    
    // Écriture des données
    encoder.write_all(data)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Zopfli write error: {}", e)))?;
    
    // Finalisation - finish() retourne le writer
    encoder.finish()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Zopfli finish error: {}", e)))?;
    
    Ok(compressed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_compress_decompress() -> io::Result<()> {
        let test_file = env::temp_dir().join("zexe_test");
        fs::write(&test_file, b"#!/bin/sh\necho 'Hello World'\n")?;
        
        let mut perms = fs::metadata(&test_file)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&test_file, perms)?;

        compress_file(&test_file)?;
        assert!(is_compressed(&test_file)?);

        // Test execution of compressed file
        use std::process::Command;
        let output = Command::new(&test_file).output()?;
        assert!(output.status.success());
        assert_eq!(output.stdout, b"Hello World\n");

        decompress_file(&test_file)?;
        assert!(!is_compressed(&test_file)?);

        fs::remove_file(&test_file)?;
        fs::remove_file(test_file.with_extension("~"))?;
        Ok(())
    }

    #[test]
    fn test_zopfli_compression() -> io::Result<()> {
        let test_data = b"Hello world! This is a test string that should compress well. ".repeat(100);
        let compressed = compress_zopfli(&test_data)?;
        
        // Decompress with flate2 to verify
        let mut decoder = GzDecoder::new(&compressed[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)?;
        
        assert_eq!(test_data.to_vec(), decompressed);
        
        println!("Zopfli compression test: {} -> {} bytes ({:.1}% ratio)", 
                 test_data.len(), compressed.len(),
                 (test_data.len() - compressed.len()) as f64 * 100.0 / test_data.len() as f64);
        
        Ok(())
    }
}

