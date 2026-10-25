//! Packt einen Ordner als `.forgeproj`, ohne die App. Zum Ausprobieren und für Testdateien:
//!
//!     cargo run -p forge-package --example pack -- <Ordner> <Ziel.forgeproj> [fqbn]

use forge_package::{pack, AiInfo, Board, Kind, Manifest, PackRequest, Toolchain, FORMAT};
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(root), Some(dest)) = (
        args.next().map(PathBuf::from),
        args.next().map(PathBuf::from),
    ) else {
        eprintln!("Aufruf: pack <Ordner> <Ziel.forgeproj> [fqbn]");
        std::process::exit(2);
    };
    let fqbn = args.next().unwrap_or_else(|| "esp32:esp32:esp32s3".into());
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "projekt".into());
    let manifest = Manifest {
        format: FORMAT,
        kind: Kind::Sketch,
        id: format!("beispiel-{name}"),
        name,
        description: String::new(),
        app: "Forge pack-Beispiel".into(),
        saved_at: 0,
        board: Board {
            fqbn,
            options: Default::default(),
        },
        toolchain: Toolchain::default(),
        libraries: vec![],
        secrets: vec![],
        ai: AiInfo {
            enabled: true,
            cached: false,
        },
    };
    match pack(
        PackRequest {
            manifest,
            root: &root,
            embedded: vec![],
        },
        &dest,
    ) {
        Ok(r) => println!("{} Dateien, {} → {} Bytes", r.files, r.bytes, r.packed),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
