//! Erzeugt ein NVS-Image (ESP-IDF, Format V2) mit Textwerten in einem Namensraum.
//! Die Umsetzung ist gegen das offizielle `nvs_partition_gen` geprüft (siehe Test).

use crc32fast::Hasher;

pub const PAGE_SIZE: usize = 4096;
const ENTRY_SIZE: usize = 32;
const ENTRIES_PER_PAGE: usize = 126;
const HEADER_SIZE: usize = 32;
const STATE_BITMAP_SIZE: usize = 32;
const TYPE_U8: u8 = 0x01;
const TYPE_STRING: u8 = 0x21;
const VERSION_2: u8 = 0xFE;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum NvsError {
    #[error("Der Name „{0}“ ist zu lang. NVS erlaubt höchstens 15 Zeichen.")]
    KeyTooLong(String),
    #[error("Der Name „{0}“ ist leer oder enthält Zeichen, die NVS nicht erlaubt.")]
    BadKey(String),
    #[error("Der Wert von „{0}“ ist zu lang. NVS speichert Texte bis 4000 Zeichen.")]
    ValueTooLong(String),
    #[error("Die Secrets passen nicht in die NVS-Partition. Nutze ein Partitionsschema mit größerer NVS-Partition.")]
    NoSpace,
    #[error("Die NVS-Partition ist mit {0} Byte zu klein. Sie braucht mindestens zwei Seiten (8192 Byte).")]
    PartitionTooSmall(usize),
}

/// zlib-CRC mit Startwert 0xFFFFFFFF, wie ESP-IDF ihn für NVS verwendet.
fn crc(data: &[u8]) -> u32 {
    let mut h = Hasher::new_with_initial(0xFFFF_FFFF);
    h.update(data);
    h.finalize()
}

fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.is_ascii() && !key.contains('\0')
}

struct Page {
    bytes: Vec<u8>,
    used: usize,
}

impl Page {
    fn new() -> Self {
        let mut bytes = vec![0xFF; PAGE_SIZE];
        bytes[0..4].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes()); // aktiv
        bytes[4..8].copy_from_slice(&0u32.to_le_bytes()); // Folgenummer
        bytes[8] = VERSION_2;
        let header_crc = crc(&bytes[4..28]);
        bytes[28..32].copy_from_slice(&header_crc.to_le_bytes());
        Page { bytes, used: 0 }
    }

    fn free(&self) -> usize {
        ENTRIES_PER_PAGE - self.used
    }

    fn set_written(&mut self, index: usize) {
        let byte = HEADER_SIZE + index / 4;
        let shift = (index % 4) * 2;
        self.bytes[byte] &= !(0b01 << shift); // 11 -> 10
    }

    fn entry_offset(index: usize) -> usize {
        HEADER_SIZE + STATE_BITMAP_SIZE + index * ENTRY_SIZE
    }

    /// Schreibt einen Kopfeintrag samt Folgeeinträgen mit Rohdaten.
    fn push(&mut self, ns: u8, kind: u8, key: &str, data8: [u8; 8], payload: &[u8]) {
        let span = 1 + payload.len().div_ceil(ENTRY_SIZE);
        let at = Self::entry_offset(self.used);
        let e = &mut self.bytes[at..at + ENTRY_SIZE];
        e[0] = ns;
        e[1] = kind;
        e[2] = span as u8;
        e[3] = 0xFF;
        e[8..24].fill(0);
        e[8..8 + key.len()].copy_from_slice(key.as_bytes());
        e[24..32].copy_from_slice(&data8);
        let mut c = Hasher::new_with_initial(0xFFFF_FFFF);
        c.update(&e[0..4]);
        c.update(&e[8..32]);
        let entry_crc = c.finalize();
        e[4..8].copy_from_slice(&entry_crc.to_le_bytes());

        for i in 0..span {
            self.set_written(self.used + i);
        }
        for (i, chunk) in payload.chunks(ENTRY_SIZE).enumerate() {
            let off = Self::entry_offset(self.used + 1 + i);
            self.bytes[off..off + chunk.len()].copy_from_slice(chunk);
        }
        self.used += span;
    }
}

/// Baut die NVS-Partition. `size` ist die Größe der Partition aus der Partitionstabelle.
/// Unbenutzte Seiten bleiben gelöscht (0xFF), wie es ESP-IDF erwartet.
pub fn build(
    namespace: &str,
    entries: &[(String, String)],
    size: usize,
) -> Result<Vec<u8>, NvsError> {
    if size < 2 * PAGE_SIZE {
        return Err(NvsError::PartitionTooSmall(size));
    }
    for k in std::iter::once(namespace).chain(entries.iter().map(|(k, _)| k.as_str())) {
        if k.len() > 15 {
            return Err(NvsError::KeyTooLong(k.to_string()));
        }
        if !valid_key(k) {
            return Err(NvsError::BadKey(k.to_string()));
        }
    }

    let mut page = Page::new();
    let mut ns_data = [0xFF; 8];
    ns_data[0] = 1;
    page.push(0, TYPE_U8, namespace, ns_data, &[]);

    for (key, value) in entries {
        if value.len() > 4000 {
            return Err(NvsError::ValueTooLong(key.clone()));
        }
        let mut payload = value.as_bytes().to_vec();
        payload.push(0);
        let needed = 1 + payload.len().div_ceil(ENTRY_SIZE);
        if needed > page.free() {
            return Err(NvsError::NoSpace);
        }
        let mut data8 = [0xFF; 8];
        data8[0..2].copy_from_slice(&(payload.len() as u16).to_le_bytes());
        data8[4..8].copy_from_slice(&crc(&payload).to_le_bytes());
        page.push(1, TYPE_STRING, key, data8, &payload);
    }

    let mut out = vec![0xFF; size];
    out[..PAGE_SIZE].copy_from_slice(&page.bytes);
    Ok(out)
}

/// Liest die Textwerte aus einem Image. Dient Tests und der Prüfung nach dem Schreiben.
pub fn read_strings(image: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if image.len() < PAGE_SIZE {
        return out;
    }
    let mut i = 0;
    while i < ENTRIES_PER_PAGE {
        let state = (image[HEADER_SIZE + i / 4] >> ((i % 4) * 2)) & 0b11;
        if state != 0b10 {
            i += 1;
            continue;
        }
        let at = Page::entry_offset(i);
        let e = &image[at..at + ENTRY_SIZE];
        let span = e[2] as usize;
        if e[0] == 1 && e[1] == TYPE_STRING {
            let key = String::from_utf8_lossy(&e[8..24])
                .trim_end_matches('\0')
                .to_string();
            let len = u16::from_le_bytes([e[24], e[25]]) as usize;
            let start = Page::entry_offset(i + 1);
            let raw = &image[start..start + len.saturating_sub(1)];
            out.push((key, String::from_utf8_lossy(raw).into_owned()));
        }
        i += span.max(1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case1() -> Vec<(String, String)> {
        vec![
            ("WIFI_SSID".into(), "Heimnetz".into()),
            ("WIFI_PASS".into(), "correct horse battery staple".into()),
            (
                "API_TOKEN".into(),
                "sk-0123456789abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"
                    .into(),
            ),
            ("EMPTY".into(), "x".into()),
        ]
    }

    /// Byte für Byte identisch mit der Ausgabe von esp-idf-nvs-partition-gen (V2).
    #[test]
    fn matches_the_official_generator() {
        let golden = include_bytes!("../tests/fixtures/nvs-v2-case1.bin");
        let ours = build("forge", &case1(), 0x5000).unwrap();
        assert_eq!(ours.len(), golden.len());
        assert_eq!(&ours[..], &golden[..]);
    }

    #[test]
    fn roundtrip_and_unused_pages_stay_erased() {
        let img = build("forge", &case1(), 0x6000).unwrap();
        assert_eq!(read_strings(&img), case1());
        assert!(img[PAGE_SIZE..].iter().all(|b| *b == 0xFF));
    }

    #[test]
    fn rejects_bad_input() {
        let one = |k: &str, v: &str| vec![(k.to_string(), v.to_string())];
        assert_eq!(
            build("forge", &one("EIN_SEHR_LANGER_NAME", "x"), 0x5000),
            Err(NvsError::KeyTooLong("EIN_SEHR_LANGER_NAME".into()))
        );
        assert_eq!(
            build("forge", &one("", "x"), 0x5000),
            Err(NvsError::BadKey("".into()))
        );
        assert_eq!(
            build("forge", &one("ÄÖ", "x"), 0x5000),
            Err(NvsError::BadKey("ÄÖ".into()))
        );
        assert_eq!(
            build("forge", &one("A", &"x".repeat(4001)), 0x5000),
            Err(NvsError::ValueTooLong("A".into()))
        );
        assert_eq!(
            build("forge", &[], 0x1000),
            Err(NvsError::PartitionTooSmall(0x1000))
        );
        let many: Vec<_> = (0..60).map(|i| (format!("K{i}"), "y".repeat(40))).collect();
        assert_eq!(build("forge", &many, 0x5000), Err(NvsError::NoSpace));
    }
}
