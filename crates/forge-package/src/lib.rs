//! Das Dateiformat `.forgeproj`: ein ganzes Forge-Projekt oder ein Sketch in einer einzigen Datei.
//!
//! Die Datei ist ein ZIP-Archiv (Deflate) mit festem Aufbau. Sie lässt sich daher auch ohne Forge mit
//! jedem Entpacker öffnen, und die Vorschau in macOS kann einzelne Dateien direkt herausgeben.
//!
//! ```text
//! manifest.json            Beschreibung (siehe [`Manifest`]), immer der erste Eintrag
//! files/…                  Inhalt des Projektordners
//! libraries/<dir>/…        Bibliotheken, die es nicht im Bibliotheksverwalter gibt
//! cache/ai-review.json     gespeicherte KI-Prüfungen
//! ai/threads.json          Gespräche über Hinweise der KI (offen und geschlossen)
//! ```
//!
//! Dasselbe Projekt ergibt immer dieselben Bytes, abgesehen vom Zeitstempel im Manifest.

mod error;
pub mod libs;
mod manifest;
mod pack;
mod read;

pub use error::{PackageError, Result};
pub use manifest::{AiInfo, Board, Kind, Library, LibrarySource, Manifest, Toolchain};
pub use pack::{pack, PackReport, PackRequest};
pub use read::{digest, extract_library, inspect, read_entry, unpack, Area, Entry, Overview};

pub const EXTENSION: &str = "forgeproj";
/// Stand des Formats. Steigt nur bei Änderungen, die ältere Versionen nicht mehr lesen können.
pub const FORMAT: u32 = 1;

pub(crate) const MANIFEST: &str = "manifest.json";
pub(crate) const FILES: &str = "files/";
pub(crate) const LIBRARIES: &str = "libraries/";
pub(crate) const AI_CACHE: &str = "cache/ai-review.json";
/// Wo der Zwischenspeicher im entpackten Projektordner liegt.
pub const AI_CACHE_FILE: &str = ".forge/ai-cache.json";
pub(crate) const AI_THREADS: &str = "ai/threads.json";
/// Wo die Gespräche über Hinweise der KI im entpackten Projektordner liegen.
pub const AI_THREADS_FILE: &str = ".forge/ai-threads.json";

/// Grenze für Entpacken und Packen, damit ein versehentlich riesiger Ordner oder ein
/// Archiv-Bombe nicht die Platte füllt.
pub(crate) const MAX_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_ENTRIES: usize = 50_000;

/// Ob ein Pfad wie ein Forge-Paket benannt ist.
pub fn is_package(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::Write;
    use std::path::Path;

    fn manifest(kind: Kind) -> Manifest {
        Manifest {
            format: FORMAT,
            kind,
            id: "abc123".into(),
            name: "demo".into(),
            description: String::new(),
            app: "Forge test".into(),
            saved_at: 0,
            board: Board {
                fqbn: "esp32:esp32:esp32s3".into(),
                options: BTreeMap::from([("FlashSize".into(), "16M".into())]),
            },
            toolchain: Toolchain::default(),
            libraries: vec![],
            secrets: vec!["WIFI_PASS".into()],
            ai: AiInfo {
                enabled: true,
                cached: false,
            },
        }
    }

    fn project(dir: &Path) {
        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::write(dir.join("demo.ino"), "void setup(){}\nvoid loop(){}\n").unwrap();
        std::fs::write(dir.join("data/index.html"), "<h1>Hallo</h1>".repeat(200)).unwrap();
        std::fs::write(dir.join("data/logo.png"), [0x89, b'P', b'N', b'G']).unwrap();
    }

    fn req<'a>(root: &'a Path, m: Manifest) -> PackRequest<'a> {
        PackRequest {
            manifest: m,
            root,
            embedded: vec![],
        }
    }

    #[test]
    fn roundtrip_keeps_files_and_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("demo");
        project(&root);
        let out = tmp.path().join("demo.forgeproj");
        let report = pack(req(&root, manifest(Kind::Project)), &out).unwrap();
        assert_eq!(report.files, 3);
        assert!(report.packed < report.bytes, "Text muss kleiner werden");

        let dest = tmp.path().join("entpackt");
        let m = unpack(&out, &dest).unwrap();
        assert_eq!(m.name, "demo");
        assert_eq!(m.board.options["FlashSize"], "16M");
        assert_eq!(m.secrets, vec!["WIFI_PASS"]);
        assert_eq!(
            std::fs::read(dest.join("demo.ino")).unwrap(),
            std::fs::read(root.join("demo.ino")).unwrap()
        );
        assert!(dest.join("data/index.html").is_file());
    }

    #[test]
    fn same_input_gives_same_bytes_except_timestamp_and_junk_is_left_out() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("demo");
        project(&root);
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join(".git/HEAD"), "ref").unwrap();
        std::fs::create_dir_all(root.join("build")).unwrap();
        std::fs::write(root.join("build/x.bin"), "x").unwrap();
        std::fs::write(root.join(".DS_Store"), "x").unwrap();
        std::fs::write(root.join("demo.ino.forge-tmp"), "x").unwrap();

        let (a, b) = (
            tmp.path().join("a.forgeproj"),
            tmp.path().join("b.forgeproj"),
        );
        pack(req(&root, manifest(Kind::Sketch)), &a).unwrap();
        pack(req(&root, manifest(Kind::Sketch)), &b).unwrap();
        let names = |p: &Path| -> Vec<String> {
            inspect(p)
                .unwrap()
                .entries
                .into_iter()
                .map(|e| e.name)
                .collect()
        };
        assert_eq!(names(&a), names(&b));
        assert_eq!(
            names(&a),
            vec![
                "manifest.json",
                "files/data/index.html",
                "files/data/logo.png",
                "files/demo.ino"
            ]
        );
        // Dateien (ohne das Manifest mit dem Zeitstempel) sind byte-gleich.
        for e in ["files/demo.ino", "files/data/index.html"] {
            assert_eq!(read_entry(&a, e).unwrap(), read_entry(&b, e).unwrap());
        }
    }

    #[test]
    fn ai_cache_travels_in_its_own_place() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("demo");
        project(&root);
        std::fs::create_dir_all(root.join(".forge")).unwrap();
        std::fs::write(root.join(AI_CACHE_FILE), "{\"k\":1}").unwrap();
        let out = tmp.path().join("d.forgeproj");
        pack(req(&root, manifest(Kind::Project)), &out).unwrap();

        let o = inspect(&out).unwrap();
        assert!(o.manifest.ai.cached);
        assert!(o
            .entries
            .iter()
            .any(|e| e.name == "cache/ai-review.json" && e.area == Area::Cache));
        let dest = tmp.path().join("neu");
        unpack(&out, &dest).unwrap();
        assert_eq!(
            std::fs::read(dest.join(AI_CACHE_FILE)).unwrap(),
            b"{\"k\":1}"
        );
    }

    #[test]
    fn conversations_with_the_ai_travel_with_the_project() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("demo");
        project(&root);
        std::fs::create_dir_all(root.join(".forge")).unwrap();
        std::fs::write(root.join(AI_THREADS_FILE), "[{\"id\":\"a\"}]").unwrap();
        let out = tmp.path().join("d.forgeproj");
        pack(req(&root, manifest(Kind::Project)), &out).unwrap();
        assert!(inspect(&out)
            .unwrap()
            .entries
            .iter()
            .any(|e| e.name == "ai/threads.json"));
        let dest = tmp.path().join("neu");
        unpack(&out, &dest).unwrap();
        assert_eq!(
            std::fs::read(dest.join(AI_THREADS_FILE)).unwrap(),
            b"[{\"id\":\"a\"}]"
        );
    }

    #[test]
    fn empty_ai_cache_is_not_packed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("demo");
        project(&root);
        std::fs::create_dir_all(root.join(".forge")).unwrap();
        std::fs::write(root.join(AI_CACHE_FILE), "{}").unwrap();
        let out = tmp.path().join("d.forgeproj");
        pack(req(&root, manifest(Kind::Project)), &out).unwrap();
        let o = inspect(&out).unwrap();
        assert!(!o.manifest.ai.cached);
        assert!(!o.entries.iter().any(|e| e.area == Area::Cache));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_never_packed_and_exec_bit_survives() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("demo");
        project(&root);
        let secret = tmp.path().join("geheim.txt");
        std::fs::write(&secret, "passwort").unwrap();
        std::os::unix::fs::symlink(&secret, root.join("link.txt")).unwrap();
        std::fs::write(root.join("flash.sh"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(
            root.join("flash.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();

        let out = tmp.path().join("d.forgeproj");
        pack(req(&root, manifest(Kind::Project)), &out).unwrap();
        assert!(!inspect(&out)
            .unwrap()
            .entries
            .iter()
            .any(|e| e.name.contains("link.txt")));
        let dest = tmp.path().join("neu");
        unpack(&out, &dest).unwrap();
        let mode = std::fs::metadata(dest.join("flash.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert!(mode & 0o111 != 0);
    }

    #[test]
    fn embedded_libraries_are_packed_and_extracted_on_demand() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("demo");
        project(&root);
        let lib = tmp.path().join("MeineLib");
        std::fs::create_dir_all(lib.join("src")).unwrap();
        std::fs::write(
            lib.join("library.properties"),
            "name=MeineLib\nversion=0.3.0\n",
        )
        .unwrap();
        std::fs::write(lib.join("src/MeineLib.h"), "#pragma once\n").unwrap();

        let mut m = manifest(Kind::Project);
        m.libraries = vec![
            Library {
                name: "MeineLib".into(),
                version: "0.3.0".into(),
                source: LibrarySource::Embedded,
                dir: Some("MeineLib".into()),
            },
            Library {
                name: "Adafruit NeoPixel".into(),
                version: "1.12.3".into(),
                source: LibrarySource::Registry,
                dir: None,
            },
        ];
        let out = tmp.path().join("d.forgeproj");
        pack(
            PackRequest {
                manifest: m,
                root: &root,
                embedded: vec![("MeineLib".into(), lib)],
            },
            &out,
        )
        .unwrap();

        // Beim normalen Entpacken bleibt die Bibliothek im Archiv.
        let dest = tmp.path().join("neu");
        unpack(&out, &dest).unwrap();
        assert!(!dest.join("MeineLib").exists());

        let libs = tmp.path().join("libraries");
        let dir = extract_library(&out, "MeineLib", &libs).unwrap();
        assert!(dir.join("src/MeineLib.h").is_file());
        assert!(matches!(
            extract_library(&out, "Gibtsnicht", &libs),
            Err(PackageError::MissingEntry(_))
        ));
        assert!(matches!(
            extract_library(&out, "../x", &libs),
            Err(PackageError::UnsafePath(_))
        ));
    }

    #[test]
    fn embedded_library_without_source_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("demo");
        project(&root);
        let mut m = manifest(Kind::Project);
        m.libraries = vec![Library {
            name: "X".into(),
            version: "1".into(),
            source: LibrarySource::Embedded,
            dir: Some("X".into()),
        }];
        let out = tmp.path().join("d.forgeproj");
        assert!(matches!(
            pack(req(&root, m), &out),
            Err(PackageError::MissingLibrarySource(_))
        ));
        assert!(!out.exists(), "keine halbe Datei");
    }

    /// Baut ein Archiv von Hand, so wie es ein Angreifer täte.
    fn hostile(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        zip.start_file("manifest.json", o).unwrap();
        zip.write_all(&serde_json::to_vec(&manifest(Kind::Project)).unwrap())
            .unwrap();
        for (n, data) in entries {
            zip.start_file(*n, o).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn path_traversal_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        for bad in [
            "files/../../evil.txt",
            "files//etc/passwd",
            "files/a/../../b",
        ] {
            let p = tmp.path().join("h.forgeproj");
            hostile(&p, &[(bad, b"x")]);
            let dest = tmp.path().join("ziel/inner");
            let r = unpack(&p, &dest);
            assert!(
                matches!(r, Err(PackageError::UnsafePath(_))),
                "{bad}: {r:?}"
            );
            assert!(!tmp.path().join("ziel/evil.txt").exists());
        }
    }

    #[test]
    fn newer_format_and_foreign_files_are_refused_with_clear_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("neu.forgeproj");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
        zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(br#"{"format":99,"irgendwas":"neues"}"#)
            .unwrap();
        zip.finish().unwrap();
        assert!(matches!(inspect(&p), Err(PackageError::TooNew(99))));

        let text = tmp.path().join("kein.forgeproj");
        std::fs::write(&text, "das ist kein zip").unwrap();
        assert!(matches!(inspect(&text), Err(PackageError::NotAPackage(_))));
        let plain_zip = tmp.path().join("plain.forgeproj");
        let mut z = zip::ZipWriter::new(std::fs::File::create(&plain_zip).unwrap());
        z.start_file("a.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        z.finish().unwrap();
        assert!(matches!(
            inspect(&plain_zip),
            Err(PackageError::NotAPackage(_))
        ));
    }

    #[test]
    fn unknown_manifest_fields_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("f.forgeproj");
        let mut v = serde_json::to_value(manifest(Kind::Sketch)).unwrap();
        v["zukunft"] = serde_json::json!({"x": 1});
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
        zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(v.to_string().as_bytes()).unwrap();
        zip.finish().unwrap();
        assert_eq!(inspect(&p).unwrap().manifest.kind, Kind::Sketch);
    }

    #[test]
    fn digest_changes_with_content() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("x");
        std::fs::write(&f, "a").unwrap();
        let a = digest(&f).unwrap();
        std::fs::write(&f, "b").unwrap();
        assert_ne!(a, digest(&f).unwrap());
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn extension_check() {
        assert!(is_package(Path::new("/x/Demo.forgeproj")));
        assert!(is_package(Path::new("demo.FORGEPROJ")));
        assert!(!is_package(Path::new("demo.ino")));
        assert!(!is_package(Path::new("forgeproj")));
    }
}
