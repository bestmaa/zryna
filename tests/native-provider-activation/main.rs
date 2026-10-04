//! Private activation preparation; this executable is never a public provider selector.
#![forbid(unsafe_code)]

mod handshake;
#[cfg(feature = "retained")]
mod retained;
mod worker;

use std::{env, error::Error, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--worker") {
        return worker::run(&args[2..]);
    }
    if args.len() != 3 || args[1] != "--smoke" {
        return Err("expected --smoke <owned-fixture-directory>".into());
    }
    let fixture = Path::new(&args[2]);
    let cases = handshake::smoke(fixture)?;
    #[cfg(feature = "retained")]
    let cases = {
        let mut all = cases;
        all.extend(retained::smoke(fixture)?);
        all
    };
    println!(
        "{}",
        serde_json::json!({
            "schema_version": 1,
            "provider": worker::PROVIDER,
            "provider_version": worker::VERSION,
            "passed": cases,
            "failed": [], "ignored": [],
            "retained": cfg!(feature = "retained"),
            "public_activation": false,
        })
    );
    Ok(())
}
