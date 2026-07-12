//! Symbolize guest PCs from a DEBUG EDK2 serial log.
//!
//! The DEBUG ArmVirtQemu build prints lines like:
//!
//!   add-symbol-file .../DxeCore.dll 0x4793B000
//!
//! Capture the full probe serial with `BRIDGEVM_BOOT_PROBE_SERIAL_OUT`, then run:
//!
//!   cargo run -p bridgevm-hvf --example edk2_symbolize_log -- /tmp/serial.log 0x47958818

use std::path::PathBuf;
use std::process::Command;

const MAX_SERIAL_LOG_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
struct LoadedImage {
    dll_path: PathBuf,
    debug_path: PathBuf,
    base: u64,
}

fn parse_addr(s: &str) -> Option<u64> {
    if let Some(pos) = s.find("0x").or_else(|| s.find("0X")) {
        let hex = s[pos + 2..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect::<String>();
        return u64::from_str_radix(&hex, 16).ok();
    }
    s.trim().parse().ok()
}

fn parse_hex_field(s: &str) -> Option<u64> {
    u64::from_str_radix(s.trim(), 16).ok()
}

fn debug_path_for(dll_path: &str) -> PathBuf {
    let path = PathBuf::from(dll_path);
    let mut debug_path = path.clone();
    if path.extension().and_then(|e| e.to_str()) == Some("dll") {
        debug_path.set_extension("debug");
    }
    debug_path
}

fn parse_symbol_line(line: &str) -> Option<LoadedImage> {
    let rest = line.strip_prefix("add-symbol-file ")?;
    let (path, base) = rest.rsplit_once(char::is_whitespace)?;
    let base = parse_addr(base)?;
    Some(LoadedImage {
        dll_path: PathBuf::from(path.trim()),
        debug_path: debug_path_for(path.trim()),
        base,
    })
}

fn load_images(serial_log: &str) -> Vec<LoadedImage> {
    let bytes = bridgevm_hvf::media::read_bounded_file(serial_log, MAX_SERIAL_LOG_BYTES)
        .unwrap_or_else(|e| panic!("read {serial_log}: {e}"));
    let contents = String::from_utf8(bytes)
        .unwrap_or_else(|e| panic!("read {serial_log}: serial log is not valid UTF-8: {e}"));
    let mut images = contents
        .lines()
        .filter_map(parse_symbol_line)
        .collect::<Vec<_>>();
    images.sort_by_key(|image| image.base);
    images.dedup_by(|a, b| a.base == b.base && a.dll_path == b.dll_path);
    images
}

fn image_for(images: &[LoadedImage], addr: u64) -> Option<&LoadedImage> {
    images.iter().take_while(|image| image.base <= addr).last()
}

fn text_vma(image: &LoadedImage) -> Result<u64, String> {
    let output = Command::new("aarch64-elf-objdump")
        .args(["-h", image.debug_path.to_str().unwrap()])
        .output()
        .map_err(|e| format!("run aarch64-elf-objdump: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.get(1) == Some(&".text") {
            return parse_hex_field(fields[3]).ok_or_else(|| format!("bad .text VMA in: {line}"));
        }
    }
    Err(format!(
        "no .text section in {}",
        image.debug_path.display()
    ))
}

fn addr2line(image: &LoadedImage, debug_addr: u64) -> String {
    if !image.debug_path.exists() {
        return format!("missing debug file: {}", image.debug_path.display());
    }
    let debug_addr = format!("{debug_addr:#x}");
    let output = Command::new("aarch64-elf-addr2line")
        .args([
            "-afiC",
            "-e",
            image.debug_path.to_str().unwrap(),
            &debug_addr,
        ])
        .output()
        .unwrap_or_else(|e| panic!("run aarch64-elf-addr2line: {e}"));
    if !output.status.success() {
        return String::from_utf8_lossy(&output.stderr).trim().to_string();
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 2 {
        eprintln!("usage: edk2_symbolize_log <serial.log> <addr> [addr...]");
        std::process::exit(2);
    }
    let serial_log = args.remove(0);
    let images = load_images(&serial_log);
    println!("loaded symbol images: {}", images.len());
    for raw_addr in args {
        let Some(addr) = parse_addr(&raw_addr) else {
            println!("{raw_addr}: could not parse address");
            continue;
        };
        let Some(image) = image_for(&images, addr) else {
            println!("{addr:#x}: no loaded image base <= address");
            continue;
        };
        let offset = addr - image.base;
        let text_vma = match text_vma(image) {
            Ok(vma) => vma,
            Err(e) => {
                println!("{addr:#x}: {e}");
                continue;
            }
        };
        let debug_addr = offset + text_vma;
        println!(
            "{addr:#x}: {} + {offset:#x} (debug {debug_addr:#x})",
            image
                .dll_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("<unknown>")
        );
        println!("{}", addr2line(image, debug_addr));
    }
}
