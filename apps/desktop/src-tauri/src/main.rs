// Verhindert zusätzliches Konsolenfenster in Release-Builds unter Windows (hier ohne Wirkung, macOS only).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `forge-desktop --mcp-bridge --info <Datei>` ist die stdio-Brücke für Claude Code, ohne Fenster.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--mcp-bridge") {
        forge_desktop_lib::mcp_bridge(
            args.iter()
                .position(|a| a == "--info")
                .and_then(|i| args.get(i + 1))
                .map(std::path::PathBuf::from),
        );
        return;
    }
    forge_desktop_lib::run()
}
