/*
    Parse a Rust source file and find all instances of unsafe
    code or FFI calls, printing them to stdout.
*/

use cargo_scan::scanner;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to crate directory; should contain a 'src' directory and a Cargo.toml file
    crate_path: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let results = scanner::scan_crate(&args.crate_path)?;

    if !results.unsafe_blocks.is_empty() {
        println!("=== Unsafe blocks ===");
        for bl_decl in results.unsafe_blocks {
            println!("{:?}", bl_decl);
        }
    }

    if !results.unsafe_decls.is_empty() {
        println!("=== Unsafe fn declarations ===");
        for fn_decl in results.unsafe_decls {
            println!("{:?}", fn_decl);
        }
    }

    if !results.unsafe_traits.is_empty() {
        println!("=== Unsafe trait declarations ===");
        for tr_decl in results.unsafe_traits {
            println!("{:?}", tr_decl);
        }
    }

    if !results.unsafe_impls.is_empty() {
        println!("=== Unsafe trait impls ===");
        for impl_decl in results.unsafe_impls {
            println!("{:?}", impl_decl);
        }
    }

    if !results.ffi_calls.is_empty() {
        println!("=== FFI Calls ===");
        for ffi_call in results.ffi_calls {
            println!("{:?}", ffi_call);
        }
    }

    Ok(())
}