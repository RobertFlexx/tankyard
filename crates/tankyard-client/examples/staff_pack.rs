//! Build a signed staff credential pack. Run privately; never uploads anything.
//! Seeds and passwords travel through bounded files, never arguments or logs.
//!
//!   staff_pack <creator|central-owner> <seed file> <password file> <output.zip> [extra files…]
//!
//! Extra files (a README, tankyard.toml, an icon) are added at their file
//! names and covered by the signature.
use std::{io::Read, io::Write, path::Path};
use tankyard_client::staff_pack::{Identity, assemble};
use zeroize::Zeroizing;

fn bounded(path: &Path, maximum: usize) -> Result<Zeroizing<Vec<u8>>, Box<dyn std::error::Error>> {
    let mut bytes = Zeroizing::new(Vec::new());
    std::fs::File::open(path)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(format!("{} is too large", path.display()).into());
    }
    Ok(bytes)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() < 4 {
        return Err("Expected identity, seed file, password file, output .zip, extra files".into());
    }
    let identity = args[0]
        .to_str()
        .and_then(Identity::parse)
        .ok_or("Unknown identity; use creator or central-owner")?;
    let seed = bounded(Path::new(&args[1]), 32)?;
    let seed: &[u8; 32] = seed.as_slice().try_into()?;
    let password = bounded(Path::new(&args[2]), 257)?;
    let password = std::str::from_utf8(&password)?.trim_end_matches(['\r', '\n']);
    let mut files = Vec::new();
    for extra in &args[4..] {
        let path = Path::new(extra);
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Extra files need UTF-8 names")?;
        files.push((name.to_owned(), bounded(path, 1 << 20)?.to_vec()));
    }
    let files = assemble(identity, seed, password, env!("CARGO_PKG_VERSION"), files)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut zip = zip::ZipWriter::new(options.open(&args[3])?);
    let stored = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o600);
    for (name, data) in &files {
        zip.start_file(name.as_str(), stored)?;
        zip.write_all(data)?;
    }
    zip.finish()?.flush()?;
    println!(
        "Signed {} pack written with {} files.",
        identity.title().to_lowercase(),
        files.len()
    );
    Ok(())
}
