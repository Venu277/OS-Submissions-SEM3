use std::path::PathBuf;
use std::process::exit;

fn main() {
    let mut args = std::env::args().skip(1);
    let kernel_path = match args.next() {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("Usage: builder <kernel-elf-path> <output-img-path>");
            exit(1);
        }
    };
    let out_path = match args.next() {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from("target/venos-uefi.img"),
    };

    println!("[VenOS Builder] Generating UEFI bootable disk image...");
    println!("  Kernel ELF:   {}", kernel_path.display());
    println!("  Output Image: {}", out_path.display());

    if !kernel_path.exists() {
        eprintln!("Error: Kernel binary '{}' does not exist!", kernel_path.display());
        exit(1);
    }

    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut uefi = bootloader::UefiBoot::new(&kernel_path);
    if let Err(e) = uefi.create_disk_image(&out_path) {
        eprintln!("Error creating UEFI disk image: {:?}", e);
        exit(1);
    }

    println!("[VenOS Builder] SUCCESS: Created UEFI disk image at {}", out_path.display());
}
