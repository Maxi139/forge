//! Native macOS-Menüleiste. Jeder eigene Eintrag sendet seine ID als Ereignis `menu` an das Fenster.
//! Die IDs entsprechen den Befehlen der Kommandopalette (`file.save`, `view.panel`, ...).

use tauri::menu::{
    AboutMetadataBuilder, Menu, MenuBuilder, MenuEvent, MenuItemBuilder, PredefinedMenuItem,
    SubmenuBuilder,
};
use tauri::{AppHandle, Emitter, Wry};

type Result<T> = tauri::Result<T>;

/// Die Projektseite. Dieselben Adressen stehen in `capabilities/default.json` (opener) und im Frontend.
pub const PROJECT_URL: &str = "https://github.com/maximilianschmidt/forge";
const ISSUES_URL: &str = "https://github.com/maximilianschmidt/forge/issues/new";
const DOCS_URL: &str = "https://github.com/maximilianschmidt/forge#readme";

fn item(
    app: &AppHandle,
    id: &str,
    label: &str,
    accelerator: Option<&str>,
) -> Result<tauri::menu::MenuItem<Wry>> {
    let mut b = MenuItemBuilder::with_id(id, label);
    if let Some(a) = accelerator {
        b = b.accelerator(a);
    }
    b.build(app)
}

/// Ein Eintrag, der ausgegraut startet und erst durch [`set_enabled`] verfügbar wird.
fn item_off(app: &AppHandle, id: &str, label: &str) -> Result<tauri::menu::MenuItem<Wry>> {
    MenuItemBuilder::with_id(id, label)
        .enabled(false)
        .build(app)
}

/// Schaltet Einträge der Menüleiste ein oder aus. Unbekannte IDs werden übergangen.
pub fn set_enabled(app: &AppHandle, states: &[(String, bool)]) {
    let Some(menu) = app.menu() else { return };
    fn walk(items: Vec<tauri::menu::MenuItemKind<Wry>>, states: &[(String, bool)]) {
        for it in items {
            match &it {
                tauri::menu::MenuItemKind::Submenu(s) => {
                    walk(s.items().unwrap_or_default(), states)
                }
                tauri::menu::MenuItemKind::MenuItem(m) => {
                    if let Some((_, on)) = states.iter().find(|(id, _)| m.id().0 == *id) {
                        let _ = m.set_enabled(*on);
                    }
                }
                _ => {}
            }
        }
    }
    walk(menu.items().unwrap_or_default(), states);
}

pub fn build(app: &AppHandle) -> Result<Menu<Wry>> {
    let about = PredefinedMenuItem::about(
        app,
        Some("Über Forge"),
        Some(
            AboutMetadataBuilder::new()
                .name(Some("Forge"))
                .version(Some(env!("CARGO_PKG_VERSION")))
                .copyright(Some("© 2026 Maximilian Schmidt"))
                .authors(Some(vec!["Maximilian Schmidt".to_string()]))
                .comments(Some("Entwicklungsumgebung für den ESP32"))
                .website(Some(PROJECT_URL))
                .website_label(Some("Projektseite"))
                .license(Some("MIT"))
                .build(),
        ),
    )?;
    let sep = || PredefinedMenuItem::separator(app);

    let app_menu = SubmenuBuilder::new(app, "Forge")
        .item(&about)
        .item(&item(
            app,
            "app.checkUpdate",
            "Nach Updates suchen …",
            None,
        )?)
        .separator()
        .item(&item(
            app,
            "app.settings",
            "Einstellungen …",
            Some("CmdOrCtrl+,"),
        )?)
        .separator()
        .item(&PredefinedMenuItem::services(app, Some("Dienste"))?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, Some("Forge ausblenden"))?)
        .item(&PredefinedMenuItem::hide_others(
            app,
            Some("Andere ausblenden"),
        )?)
        .item(&PredefinedMenuItem::show_all(app, Some("Alle einblenden"))?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, Some("Forge beenden"))?)
        .build()?;

    let file = SubmenuBuilder::new(app, "Datei")
        .item(&item(
            app,
            "file.newSketch",
            "Neuer Sketch",
            Some("CmdOrCtrl+N"),
        )?)
        .item(&item(
            app,
            "file.newProject",
            "Neues Projekt …",
            Some("CmdOrCtrl+Shift+N"),
        )?)
        .item(&item(
            app,
            "file.open",
            "Projektdatei öffnen …",
            Some("CmdOrCtrl+O"),
        )?)
        .item(&item(app, "file.openFolder", "Ordner öffnen …", None)?)
        .separator()
        .item(&item(app, "file.save", "Speichern", Some("CmdOrCtrl+S"))?)
        .item(
            &MenuItemBuilder::with_id("file.saveAs", "Speichern unter …")
                .accelerator("CmdOrCtrl+Shift+S")
                .enabled(false)
                .build(app)?,
        )
        .item(&item(
            app,
            "file.saveAll",
            "Alle speichern",
            Some("CmdOrCtrl+Alt+S"),
        )?)
        .separator()
        .item(&item_off(
            app,
            "file.exportFolder",
            "Als Ordner exportieren …",
        )?)
        .separator()
        .item(&item(
            app,
            "file.close",
            "Tab schließen",
            Some("CmdOrCtrl+W"),
        )?)
        .item(&PredefinedMenuItem::close_window(
            app,
            Some("Fenster schließen"),
        )?)
        .build()?;

    let edit = SubmenuBuilder::new(app, "Bearbeiten")
        .item(&item(app, "edit.undo", "Widerrufen", Some("CmdOrCtrl+Z"))?)
        .item(&item(
            app,
            "edit.redo",
            "Wiederholen",
            Some("CmdOrCtrl+Shift+Z"),
        )?)
        .item(&sep()?)
        .item(&PredefinedMenuItem::cut(app, Some("Ausschneiden"))?)
        .item(&PredefinedMenuItem::copy(app, Some("Kopieren"))?)
        .item(&PredefinedMenuItem::paste(app, Some("Einfügen"))?)
        .item(&PredefinedMenuItem::select_all(
            app,
            Some("Alles auswählen"),
        )?)
        .separator()
        .item(&item(app, "edit.find", "Suchen …", Some("CmdOrCtrl+F"))?)
        .item(&item(
            app,
            "edit.findNext",
            "Weitersuchen",
            Some("CmdOrCtrl+G"),
        )?)
        .item(&item(
            app,
            "edit.findPrevious",
            "Rückwärts suchen",
            Some("CmdOrCtrl+Shift+G"),
        )?)
        .build()?;

    let sketch = SubmenuBuilder::new(app, "Sketch")
        .item(&item(
            app,
            "build.compile",
            "Kompilieren",
            Some("CmdOrCtrl+B"),
        )?)
        .item(&item(
            app,
            "build.flash",
            "Kompilieren und flashen",
            Some("CmdOrCtrl+U"),
        )?)
        .item(&item(
            app,
            "ai.task",
            "KI-Aufgabe …",
            Some("CmdOrCtrl+Shift+I"),
        )?)
        .separator()
        .item(&item(
            app,
            "view.monitor",
            "Seriellen Monitor öffnen",
            Some("CmdOrCtrl+Shift+M"),
        )?)
        .item(&item(app, "device.rescan", "Ports neu suchen", None)?)
        .separator()
        .item(&item_off(
            app,
            "file.convertToProject",
            "Sketch als Projekt speichern",
        )?)
        .build()?;

    let view = SubmenuBuilder::new(app, "Ansicht")
        .item(&item(
            app,
            "view.search",
            "Im Projekt suchen",
            Some("CmdOrCtrl+Shift+F"),
        )?)
        .item(&item(
            app,
            "view.palette",
            "Befehlspalette …",
            Some("CmdOrCtrl+K"),
        )?)
        .item(&item(
            app,
            "file.quickOpen",
            "Datei öffnen …",
            Some("CmdOrCtrl+P"),
        )?)
        .item(&item(
            app,
            "editor.symbols",
            "Gehe zu Symbol …",
            Some("CmdOrCtrl+Shift+O"),
        )?)
        .separator()
        .item(&item(
            app,
            "view.sidebar",
            "Seitenleiste ein- oder ausblenden",
            Some("CmdOrCtrl+\\"),
        )?)
        .item(&item(
            app,
            "view.panel",
            "Unteres Panel ein- oder ausblenden",
            Some("CmdOrCtrl+J"),
        )?)
        .item(&item(
            app,
            "view.device",
            "Geräteansicht ein- oder ausblenden",
            Some("CmdOrCtrl+Alt+D"),
        )?)
        .separator()
        .item(&item(
            app,
            "view.theme",
            "Hell und Dunkel umschalten",
            None,
        )?)
        .separator()
        .item(&PredefinedMenuItem::fullscreen(app, Some("Vollbild"))?)
        .build()?;

    let window = SubmenuBuilder::new(app, "Fenster")
        .item(&PredefinedMenuItem::minimize(app, Some("Minimieren"))?)
        .item(&PredefinedMenuItem::maximize(app, Some("Zoomen"))?)
        .separator()
        .item(&PredefinedMenuItem::bring_all_to_front(
            app,
            Some("Alle in den Vordergrund"),
        )?)
        .build()?;

    // macOS hängt an das Hilfe-Menü ein Suchfeld, das alle Menüeinträge durchsucht.
    let help = SubmenuBuilder::new(app, "Hilfe")
        .item(&item(app, "help.docs", "Dokumentation", None)?)
        .item(&item(app, "help.report", "Problem melden …", None)?)
        .separator()
        .item(&item(
            app,
            "help.revealLog",
            "Protokoll im Finder zeigen",
            None,
        )?)
        .build()?;
    help.set_as_help_menu_for_nsapp()?;

    MenuBuilder::new(app)
        .items(&[&app_menu, &file, &edit, &sketch, &view, &window, &help])
        .build()
}

pub fn on_event(app: &AppHandle, event: MenuEvent) {
    let url = match event.id().0.as_str() {
        "help.revealLog" => return crate::logging::reveal(app),
        "help.docs" => Some(DOCS_URL),
        "help.report" => Some(ISSUES_URL),
        _ => None,
    };
    if let Some(url) = url {
        if let Err(e) = tauri_plugin_opener::open_url(url, None::<&str>) {
            log::warn!("Adresse lässt sich nicht öffnen ({url}): {e}");
        }
        return;
    }
    let _ = app.emit("menu", event.id().0.clone());
}
