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
    compression_level: CompressionLevel,
    iterations: Option<NonZeroU64>,
    iterations_without_improvement: Option<NonZeroU64>,
    max_block_splits: Option<u16>,
    block_type: BlockType,
    verbose: bool,
}

#[derive(Debug, Clone, Copy)]
enum CompressionLevel {
    Fast,      // Compression rapide, moins bonne
    Normal,    // Équilibre (défaut)
    Maximum,   // Bonne compression, plus lent
    Ultra,     // Compression extrême, très lent
    Custom,    // Paramètres personnalisés
}

impl CompressionLevel {
    fn as_str(&self) -> &'static str {
        match self {
            CompressionLevel::Fast => "fast",
            CompressionLevel::Normal => "normal",
            CompressionLevel::Maximum => "maximum",
            CompressionLevel::Ultra => "ultra",
            CompressionLevel::Custom => "custom",
        }
    }
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

    // CORRECTION: Itérer sur une référence avec &config.files
    for file in &config.files {
        let result = if config.decompress {
            decompress_file(file)  // Note: on passe &file directement
        } else {
            compress_file(file, &config)  // Note: on passe &file directement
        };

        match result {
            Ok(Some(info)) => {
                if config.decompress {
                    println!("{}: decompressed ({} -> {} bytes, {:.1}% saved)",
                             info.path.display(), info.compressed_size, info.original_size,
                             info.compression_ratio());
                } else {
                    println!("{}: {} -> {} bytes, {:.1}% compression (Zopfli - {})",
                             info.path.display(), info.original_size, info.compressed_size,
                             info.compression_ratio(), config.compression_level.as_str());
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
    let mut compression_level = CompressionLevel::Normal;
    let mut iterations = None;
    let mut iterations_without_improvement = None;
    let mut max_block_splits = None;
    let mut block_type = BlockType::Dynamic;
    let mut verbose = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-d" => decompress = true,
            "-1" | "--fast" => compression_level = CompressionLevel::Fast,
            "-2" | "--normal" => compression_level = CompressionLevel::Normal,
            "-3" | "--maximum" => compression_level = CompressionLevel::Maximum,
            "-4" | "--ultra" => compression_level = CompressionLevel::Ultra,
            "--custom" => {
                compression_level = CompressionLevel::Custom;
                // Les paramètres personnalisés seront lus via d'autres options
            }
            "--iterations" => {
                i += 1;
                if i >= args.len() {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput,
                        "Missing value for --iterations"));
                }
                let val = args[i].parse::<u64>()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput,
                        "Invalid number for --iterations"))?;
                if val == 0 {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput,
                        "Iterations must be > 0"));
                }
                iterations = Some(NonZeroU64::new(val).unwrap());
                compression_level = CompressionLevel::Custom;
            }
            "--iter-without-improvement" => {
                i += 1;
                if i >= args.len() {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput,
                        "Missing value for --iter-without-improvement"));
                }
                let val = args[i].parse::<u64>()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput,
                        "Invalid number for --iter-without-improvement"))?;
                if val == 0 {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput,
                        "Iterations without improvement must be > 0"));
                }
                iterations_without_improvement = Some(NonZeroU64::new(val).unwrap());
                compression_level = CompressionLevel::Custom;
            }
            "--max-block-splits" => {
                i += 1;
                if i >= args.len() {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput,
                        "Missing value for --max-block-splits"));
                }
                let val = args[i].parse::<u16>()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput,
                        "Invalid number for --max-block-splits"))?;
                max_block_splits = Some(val);
                compression_level = CompressionLevel::Custom;
            }
            "--block-type" => {
                i += 1;
                if i >= args.len() {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput,
                        "Missing value for --block-type"));
                }
                block_type = match args[i].as_str() {
                    "dynamic" => BlockType::Dynamic,
                    "fixed" => BlockType::Fixed,
                    _ => {
                        return Err(io::Error::new(io::ErrorKind::InvalidInput,
                            "Block type must be 'dynamic' or 'fixed'"));
                    }
                };
                compression_level = CompressionLevel::Custom;
            }
            "-v" | "--verbose" => verbose = true,
            "-h" | "--help" => {
                print_help(&args[0]);
                process::exit(0);
            }
            "-V" | "--version" => {
                println!("zexe version 0.2.0 (Zopfli)");
                println!("Author: {} ({}) {}", AUTHOR, YEAR, WEBSITE);
                println!("Compression levels: fast, normal (default), maximum, ultra");
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

    Ok(Config {
        decompress,
        files,
        compression_level,
        iterations,
        iterations_without_improvement,
        max_block_splits,
        block_type,
        verbose,
    })
}

fn print_help(program: &str) {
    println!("zexe - Self-extracting executable compressor");
    println!("Author: {} ({}) {}", AUTHOR, YEAR, WEBSITE);
    println!();
    println!("Usage: {} [OPTIONS] file...", program);
    println!();
    println!("Options:");
    println!("  -d                    Decompress the file");
    println!("  -1, --fast            Fast compression (lower ratio)");
    println!("  -2, --normal          Normal compression (default)");
    println!("  -3, --maximum          Maximum compression");
    println!("  -4, --ultra            Ultra compression (very slow)");
    println!("  --custom               Use custom compression parameters");
    println!("  --iterations N         Number of iterations (default varies)");
    println!("  --iter-without-improvement N");
    println!("                         Stop after N iterations without improvement");
    println!("  --max-block-splits N   Maximum number of block splits");
    println!("  --block-type TYPE      Block type: dynamic or fixed");
    println!("  -v, --verbose           Verbose output");
    println!("  -h, --help             Show this help");
    println!("  -V, --version          Show version");
    println!();
    println!("Compression levels:");
    println!("  fast:    15 iterations, 3 without improvement, 15 splits");
    println!("  normal:  30 iterations, 5 without improvement, 25 splits (default)");
    println!("  maximum: 75 iterations, 12 without improvement, 50 splits");
    println!("  ultra:   200 iterations, 30 without improvement, 100 splits");
    println!();
    println!("Examples:");
    println!("  {} myprogram            # Compress with normal settings", program);
    println!("  {} --ultra myprogram    # Maximum compression", program);
    println!("  {} -d myprogram         # Decompress", program);
    println!("  {} --iterations 100 --max-block-splits 75 myprogram", program);
}

fn get_compression_options(config: &Config) -> Options {
    match config.compression_level {
        CompressionLevel::Fast => {
            Options {
                iteration_count: NonZeroU64::new(15).unwrap(),
                iterations_without_improvement: NonZeroU64::new(3).unwrap(),
                maximum_block_splits: 15,
            }
        }
        CompressionLevel::Normal => {
            Options {
                iteration_count: NonZeroU64::new(30).unwrap(),
                iterations_without_improvement: NonZeroU64::new(5).unwrap(),
                maximum_block_splits: 25,
            }
        }
        CompressionLevel::Maximum => {
            Options {
                iteration_count: NonZeroU64::new(75).unwrap(),
                iterations_without_improvement: NonZeroU64::new(12).unwrap(),
                maximum_block_splits: 50,
            }
        }
        CompressionLevel::Ultra => {
            Options {
                iteration_count: NonZeroU64::new(200).unwrap(),
                iterations_without_improvement: NonZeroU64::new(30).unwrap(),
                maximum_block_splits: 100,
            }
        }
        CompressionLevel::Custom => {
            Options {
                iteration_count: config.iterations.unwrap_or_else(|| NonZeroU64::new(30).unwrap()),
                iterations_without_improvement: config.iterations_without_improvement.unwrap_or_else(|| NonZeroU64::new(5).unwrap()),
                maximum_block_splits: config.max_block_splits.unwrap_or(25),
            }
        }
    }
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

fn compress_file(path: &Path, config: &Config) -> io::Result<Option<FileInfo>> {
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

    // Get compression options
    let options = get_compression_options(config);
    
    if config.verbose {
        eprintln!("Compression settings:");
        eprintln!("  Iterations: {}", options.iteration_count);
        eprintln!("  Iterations without improvement: {}", options.iterations_without_improvement);
        eprintln!("  Max block splits: {}", options.maximum_block_splits);
        eprintln!("  Block type: {:?}", config.block_type);
    }

    // Compress with Zopfli
    println!("Compressing {} with Zopfli ({} level, this may take a while)...", 
             path.display(), config.compression_level.as_str());
    
    let compressed = compress_zopfli(&original_data, options, config.block_type)?;
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

    if config.verbose {
        eprintln!("Compression complete:");
        eprintln!("  Original size: {} bytes", original_size);
        eprintln!("  Compressed size: {} bytes", compressed_size + header_bytes.len() as u64);
        eprintln!("  Header size: {} bytes", header_bytes.len());
        eprintln!("  Compression ratio: {:.1}%", 
                 (original_size - compressed_size) as f64 * 100.0 / original_size as f64);
    }

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

fn compress_zopfli(data: &[u8], options: Options, block_type: BlockType) -> io::Result<Vec<u8>> {
    let mut compressed = Vec::new();
    
    // Créer l'encodeur
    let mut encoder = GzipEncoder::new(options, block_type, &mut compressed)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Zopfli init error: {}", e)))?;
    
    // Écriture des données
    encoder.write_all(data)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Zopfli write error: {}", e)))?;
    
    // Finalisation
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

        let config = Config {
            decompress: false,
            files: vec![test_file.clone()],
            compression_level: CompressionLevel::Normal,
            iterations: None,
            iterations_without_improvement: None,
            max_block_splits: None,
            block_type: BlockType::Dynamic,
            verbose: false,
        };

        compress_file(&test_file, &config)?;
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
    fn test_zopfli_compression_levels() -> io::Result<()> {
        let test_data = b"Hello world! This is a test string that should compress well. ".repeat(100);
        
        let levels = [
            CompressionLevel::Fast,
            CompressionLevel::Normal,
            CompressionLevel::Maximum,
            CompressionLevel::Ultra,
        ];

        for level in levels {
            let options = match level {
                CompressionLevel::Fast => Options {
                    iteration_count: NonZeroU64::new(15).unwrap(),
                    iterations_without_improvement: NonZeroU64::new(3).unwrap(),
                    maximum_block_splits: 15,
                },
                CompressionLevel::Normal => Options {
                    iteration_count: NonZeroU64::new(30).unwrap(),
                    iterations_without_improvement: NonZeroU64::new(5).unwrap(),
                    maximum_block_splits: 25,
                },
                CompressionLevel::Maximum => Options {
                    iteration_count: NonZeroU64::new(75).unwrap(),
                    iterations_without_improvement: NonZeroU64::new(12).unwrap(),
                    maximum_block_splits: 50,
                },
                CompressionLevel::Ultra => Options {
                    iteration_count: NonZeroU64::new(200).unwrap(),
                    iterations_without_improvement: NonZeroU64::new(30).unwrap(),
                    maximum_block_splits: 100,
                },
                CompressionLevel::Custom => unreachable!(),
            };

            let compressed = compress_zopfli(&test_data, options, BlockType::Dynamic)?;
            
            // Decompress with flate2 to verify
            let mut decoder = GzDecoder::new(&compressed[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            
            assert_eq!(test_data.to_vec(), decompressed);
            
            println!("Zopfli {:?}: {} -> {} bytes ({:.1}% ratio)", 
                     level, test_data.len(), compressed.len(),
                     (test_data.len() - compressed.len()) as f64 * 100.0 / test_data.len() as f64);
        }
        
        Ok(())
    }
}

