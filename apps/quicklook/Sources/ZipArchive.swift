import Compression
import Foundation

/// Liest ein `.forgeproj` (ZIP, Deflate). Das Format steht in `crates/forge-package`.
/// Der Leser kennt nur, was Forge schreibt: kein ZIP64, keine Verschlüsselung.
struct ZipEntry {
    let name: String
    let method: UInt16
    let compressedSize: Int
    let size: Int
    let localOffset: Int

    var isDirectory: Bool { name.hasSuffix("/") }
}

enum ZipError: LocalizedError {
    case notAnArchive
    case unsupported
    case corrupt(String)

    var errorDescription: String? {
        switch self {
        case .notAnArchive: return "Das ist keine Forge-Projektdatei oder sie ist beschädigt."
        case .unsupported: return "Diese Projektdatei nutzt ein Format, das die Vorschau nicht lesen kann."
        case .corrupt(let name): return "„\(name)“ in der Projektdatei ist beschädigt."
        }
    }
}

final class ZipArchive {
    let entries: [ZipEntry]
    private let data: Data

    init(url: URL) throws {
        data = try Data(contentsOf: url, options: .mappedIfSafe)
        entries = try ZipArchive.readDirectory(data)
    }

    private static func u16(_ d: Data, _ o: Int) -> Int {
        Int(d[d.startIndex + o]) | Int(d[d.startIndex + o + 1]) << 8
    }

    private static func u32(_ d: Data, _ o: Int) -> Int {
        u16(d, o) | u16(d, o + 2) << 16
    }

    private static func readDirectory(_ d: Data) throws -> [ZipEntry] {
        guard d.count >= 22 else { throw ZipError.notAnArchive }
        // Das Ende des Verzeichnisses steht in den letzten 22 Bytes plus einem möglichen Kommentar.
        var eocd = -1
        var i = d.count - 22
        let lowest = max(0, d.count - 22 - 65535)
        while i >= lowest {
            if u32(d, i) == 0x0605_4b50 { eocd = i; break }
            i -= 1
        }
        guard eocd >= 0 else { throw ZipError.notAnArchive }
        let count = u16(d, eocd + 10)
        let offset = u32(d, eocd + 16)
        if count == 0xFFFF || offset == 0xFFFF_FFFF { throw ZipError.unsupported }

        var out: [ZipEntry] = []
        var p = offset
        for _ in 0..<count {
            guard p + 46 <= d.count, u32(d, p) == 0x0201_4b50 else { throw ZipError.notAnArchive }
            let method = UInt16(u16(d, p + 10))
            let csize = u32(d, p + 20)
            let size = u32(d, p + 24)
            let nameLen = u16(d, p + 28)
            let extraLen = u16(d, p + 30)
            let commentLen = u16(d, p + 32)
            let local = u32(d, p + 42)
            guard p + 46 + nameLen <= d.count else { throw ZipError.notAnArchive }
            let nameData = d.subdata(in: (d.startIndex + p + 46)..<(d.startIndex + p + 46 + nameLen))
            let name = String(decoding: nameData, as: UTF8.self)
            out.append(ZipEntry(name: name, method: method, compressedSize: csize, size: size, localOffset: local))
            p += 46 + nameLen + extraLen + commentLen
        }
        return out
    }

    func entry(named name: String) -> ZipEntry? {
        entries.first { $0.name == name }
    }

    func read(_ e: ZipEntry) throws -> Data {
        let o = e.localOffset
        guard o + 30 <= data.count, ZipArchive.u32(data, o) == 0x0403_4b50 else { throw ZipError.corrupt(e.name) }
        let start = o + 30 + ZipArchive.u16(data, o + 26) + ZipArchive.u16(data, o + 28)
        guard start + e.compressedSize <= data.count else { throw ZipError.corrupt(e.name) }
        let packed = data.subdata(in: (data.startIndex + start)..<(data.startIndex + start + e.compressedSize))
        switch e.method {
        case 0:
            return packed
        case 8:
            if e.size == 0 { return Data() }
            var out = Data(count: e.size)
            let written = out.withUnsafeMutableBytes { dst in
                packed.withUnsafeBytes { src in
                    compression_decode_buffer(
                        dst.bindMemory(to: UInt8.self).baseAddress!, e.size,
                        src.bindMemory(to: UInt8.self).baseAddress!, packed.count,
                        nil, COMPRESSION_ZLIB)
                }
            }
            guard written == e.size else { throw ZipError.corrupt(e.name) }
            return out
        default:
            throw ZipError.unsupported
        }
    }
}

/// Der Pfad einer Datei, wenn sie sicher unter einem Zielordner landen darf. Nie absolut, nie mit `..`.
func safeRelativePath(_ raw: String) -> String? {
    if raw.isEmpty || raw.hasPrefix("/") || raw.contains("\\") { return nil }
    let parts = raw.split(separator: "/", omittingEmptySubsequences: false)
    if parts.contains(where: { $0 == ".." || $0 == "." || $0.isEmpty }) { return nil }
    return raw
}
