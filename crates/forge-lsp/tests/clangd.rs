//! Echter Lauf mit clangd, arduino-cli und dem ESP32-Core. Läuft nur, wenn alles da ist.

use forge_lsp::{Lsp, StartOptions, Template};
use forge_toolchain::check::{CheckRequest, Checker};
use forge_toolchain::cli::ArduinoCli;
use forge_toolchain::tools::Tools;
use forge_toolchain::CancellationToken;
use std::collections::HashMap;
use std::path::PathBuf;

#[tokio::test]
#[ignore = "braucht clangd, arduino-cli und den ESP32-Core"]
async fn completion_hover_definition_and_tokens_map_back_to_the_sketch() {
    let clangd = PathBuf::from("/usr/bin/clangd");
    let Ok(tools) = Tools::locate(&[]) else {
        return;
    };
    if !clangd.exists() {
        return;
    }
    let cli = ArduinoCli::new(tools.arduino_cli);
    let fqbn = "esp32:esp32:esp32";
    if cli.board_options(fqbn).await.is_err() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("probe");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let ino = root.join("probe.ino");
    let text = "int zaehler = 0;\n\nvoid setup() {\n  Serial.begin(115200);\n  pinMode(2, OUTPUT);\n}\n\nvoid loop() {\n  digitalWrite(2, zaehler % 2);\n  zaehler++;\n}\n";
    std::fs::write(&ino, text).unwrap();

    let checker = Checker::new(cli.clone(), tmp.path().join("cache"));
    let cancel = CancellationToken::new();
    let t = checker
        .flags_template(
            &CheckRequest {
                root: root.clone(),
                fqbn: fqbn.into(),
                overrides: HashMap::new(),
                libraries_signature: String::new(),
            },
            &cancel,
        )
        .await
        .unwrap()
        .expect("Compile-Datenbank");
    let lsp = Lsp::start(StartOptions {
        clangd,
        work: tmp.path().join("lsp"),
        root: root.clone(),
        template: Template {
            directory: t.directory,
            arguments: t.arguments,
            template_file: t.template_file,
            shadow: t.shadow,
            main_name: t.main_name,
        },
    })
    .await
    .unwrap();
    lsp.sync(&[(ino.clone(), text.to_string())]).await.unwrap();
    let file = ino.to_string_lossy().to_string();

    // Nach „Serial.“ in Zeile 4 kommen Methoden von HardwareSerial.
    let items = lsp.complete(&file, 4, 9).await.unwrap();
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"begin"), "{labels:?}");

    let hover = lsp.hover(&file, 9, 4).await.unwrap().expect("Hover");
    assert!(hover.text.contains("digitalWrite"), "{}", hover.text);

    // Definition von `zaehler` in Zeile 9 liegt in Zeile 1 derselben Datei, nicht im zusammengesetzten Text.
    let defs = lsp.definition(&file, 9, 20).await.unwrap();
    assert_eq!(defs.len(), 1, "{defs:?}");
    assert_eq!(
        (defs[0].file.as_str(), defs[0].line, defs[0].external),
        (file.as_str(), 1, false)
    );

    // Definition einer Core-Funktion liegt außerhalb des Projekts.
    let core = lsp.definition(&file, 9, 4).await.unwrap();
    assert!(
        core.first()
            .is_some_and(|l| l.external && l.file.ends_with("esp32-hal-gpio.h")),
        "{core:?}"
    );

    let sig = lsp
        .signature(&file, 5, 12)
        .await
        .unwrap()
        .expect("Signatur");
    assert!(sig.label.contains("pinMode"), "{}", sig.label);

    let tokens = lsp.tokens(&file).await.unwrap();
    assert!(!tokens.types.is_empty());
    assert!(
        tokens.tokens.iter().any(|t| t.line == 1),
        "Zeile 1 hat Token"
    );
    assert!(
        tokens.tokens.iter().all(|t| t.line >= 1 && t.line <= 11),
        "alle Token liegen in der Datei"
    );
}
