//! Local packaging utility. Passwords travel through bounded files, never arguments or logs.
use std::{io::Read, path::Path};
use zeroize::Zeroizing;
fn bounded(path: &Path, maximum: usize) -> Result<Zeroizing<Vec<u8>>, Box<dyn std::error::Error>> {
    let mut bytes = Zeroizing::new(Vec::new());
    std::fs::File::open(path)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err("Credential input is too large".into());
    }
    Ok(bytes)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 4 {
        return Err("Expected identity, seed file, password file, output file".into());
    }
    let expected = match args[0].to_str() {
        Some("creator") => tankyard_protocol::CREATOR_PUBLIC_KEY,
        Some("central-owner") => tankyard_protocol::CENTRAL_OWNER_PUBLIC_KEY,
        _ => return Err("Unknown identity".into()),
    };
    let seed = bounded(Path::new(&args[1]), 32)?;
    let seed: &[u8; 32] = seed.as_slice().try_into()?;
    let password = bounded(Path::new(&args[2]), 257)?;
    let password = std::str::from_utf8(&password)?.trim_end_matches(['\r', '\n']);
    let sealed = tankyard_client::credential_vault::seal(seed, password, &expected)
        .map_err(|_| "Unable to seal credential")?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    options.open(&args[3])?.write_all(&sealed)?;
    println!("Encrypted credential prepared.");
    Ok(())
}
