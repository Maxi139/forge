import Foundation

/// Spiegel von `manifest.json` (siehe `crates/forge-package/src/manifest.rs`). Unbekannte Felder werden ignoriert.
struct Manifest: Decodable {
    struct Board: Decodable {
        let fqbn: String
        let options: [String: String]?
    }

    struct Library: Decodable {
        let name: String
        let version: String
        let source: String
    }

    struct Toolchain: Decodable {
        let cores: [String: String]?
    }

    struct AI: Decodable {
        let enabled: Bool
        let cached: Bool?
    }

    let format: Int
    let kind: String
    let name: String
    let description: String?
    let app: String?
    let savedAt: Double?
    let board: Board
    let toolchain: Toolchain?
    let libraries: [Library]?
    let secrets: [String]?
    let ai: AI?
}
