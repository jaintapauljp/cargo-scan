//! Utility functions

/// Initialize logging for all of cargo_scan
///
/// To change the log level, run with e.g.:
/// RUST_LOG=debug cargo run --bin scan ...
/// RUST_LOG=info cargo run --bin scan ...
pub fn init_logging() {
    use env_logger::Builder;
    use std::env;

    // wish there was a nicer way to do this, env_logger doesn't make it easy
    // to disable non-cargo_scan logging
    let filters = "warn,cargo_scan=".to_string()
        + env::var("RUST_LOG").as_deref().unwrap_or("warn");

    Builder::new().parse_filters(&filters).init();
}

/// CSV utility functions
pub mod csv {
    use log::warn;
    use std::path::Path;

    pub fn sanitize_comma(s: &str) -> String {
        if s.contains(',') {
            warn!("Warning: ignoring unexpected comma when generating CSV: {s}");
        }
        s.replace(',', "")
    }
    pub fn sanitize_path(p: &Path) -> String {
        match p.to_str() {
            Some(s) => sanitize_comma(s),
            None => {
                warn!("Warning: path is invalid unicode: {:?}", p);
                sanitize_comma(&p.to_string_lossy())
            }
        }
    }
}

/// Iterator util
pub mod iter {
    use log::warn;
    use std::fmt::Display;
    use std::vec;

    /// Ignore errors, printing them to stderr
    /// useful with iter::filter_map: `my_iter.filter_map(warn_ok)`
    pub fn warn_ok<T, E: Display>(x: Result<T, E>) -> Option<T> {
        if let Some(e) = x.as_ref().err() {
            warn!("Warning: discarding error {}", e);
        }
        x.ok()
    }

    /// Convert an iterator into one that owns all its elements
    pub trait FreshIter {
        type Result: Iterator;
        fn fresh_iter(self) -> Self::Result;
    }
    impl<I: Iterator> FreshIter for I {
        type Result = vec::IntoIter<I::Item>;
        fn fresh_iter(self) -> Self::Result {
            self.collect::<Vec<I::Item>>().into_iter()
        }
    }
}

/// Filesystem util
pub mod fs {
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::path::PathBuf;
    use walkdir::{DirEntry, WalkDir};

    pub fn walk_files(p: &PathBuf) -> impl Iterator<Item = PathBuf> {
        debug_assert!(p.is_dir());
        WalkDir::new(p)
            .sort_by_file_name()
            .into_iter()
            .filter_map(super::iter::warn_ok)
            .map(DirEntry::into_path)
    }

    pub fn walk_files_with_extension<'a>(
        p: &'a PathBuf,
        ext: &'a str,
    ) -> impl Iterator<Item = PathBuf> + 'a {
        walk_files(p)
            .filter(|entry| entry.is_file())
            .filter(|entry| entry.extension().map_or(false, |x| x.to_str() == Some(ext)))
    }

    pub fn file_lines(p: &PathBuf) -> impl Iterator<Item = String> {
        let file = File::open(p).unwrap();
        let reader = BufReader::new(file).lines();
        reader.map(|line| line.unwrap())
    }
}

/// Parse Cargo TOML
use anyhow::{anyhow, Context, Result};
use log::debug;
use std::fs::read_to_string;
use std::path::Path;
use std::str::FromStr;
use toml::{self, value::Table};

#[derive(Debug, Clone)]
pub struct CrateData {
    pub name: String,
    pub version: String,
}

pub fn load_cargo_toml(crate_path: &Path) -> Result<CrateData> {
    debug!("Loading Cargo.toml at: {:?}", crate_path);

    let toml_string = read_to_string(crate_path.join("Cargo.toml"))?;
    let cargo_toml =
        toml::from_str::<Table>(&toml_string).context("Couldn't parse Cargo.toml")?;
    let root_toml_table = cargo_toml
        .get("package")
        .context("No package in Cargo.toml")?
        .as_table()
        .context("Package field is not a table")?;
    let name = root_toml_table
        .get("name")
        .context("No name for the root package in Cargo.toml")?
        .as_str()
        .context("name field in package is not a string")?
        .to_string();
    let version = root_toml_table
        .get("version")
        .context("No version for the root package in Cargo.toml")?
        .as_str()
        .context("version field in package couldn't be interpreted as a string")?
        .to_string();

    let result = CrateData { name, version };
    debug!("Loaded: {:?}", result);
    Ok(result)
}

/// Takes a fully-stringified crate name and gets the name and version from it
/// e.g. "libc-0.4.8-pre18" will return "libc" and the version 0.4.8 with
/// prerelease string "pre18"
pub fn package_info_from_string(
    package: &str,
) -> Result<(cargo_lock::Name, cargo_lock::Version)> {
    let split = package.split('-').collect::<Vec<_>>();
    for i in (0..split.len()).rev() {
        let test_str = split[i..].join("-");
        if let Ok(version) = cargo_lock::Version::parse(&test_str) {
            let name = split[..i].join("-");
            return Ok((cargo_lock::Name::from_str(&name)?, version));
        }
    }

    Err(anyhow!("Couldn't parse string as package"))
}
