use std::cell::RefCell;
use tree_sitter::{Parser, Tree};

thread_local! {
    static PARSER: RefCell<Parser> = RefCell::new({
        let mut p = Parser::new();
        p.set_language(&tree_sitter_cpp::LANGUAGE.into()).expect("tree-sitter-cpp passt zu tree-sitter");
        p
    });
}

/// Bekannte Attribut-Makros der ESP32-Core. tree-sitter kennt sie nicht und meldet sonst Fehler.
const ATTRIBUTE_MACROS: &[&str] = &[
    "IRAM_ATTR",
    "DRAM_ATTR",
    "RTC_IRAM_ATTR",
    "RTC_DATA_ATTR",
    "RTC_NOINIT_ATTR",
    "RTC_FAST_ATTR",
    "RTC_SLOW_ATTR",
    "EXT_RAM_ATTR",
    "EXT_RAM_BSS_ATTR",
    "DMA_ATTR",
    "WORD_ALIGNED_ATTR",
    "NOINLINE_ATTR",
    "FORCE_INLINE_ATTR",
    "ICACHE_RAM_ATTR",
    "ICACHE_FLASH_ATTR",
    "PROGMEM",
];

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Ersetzt Attribut-Makros durch gleich viele Leerzeichen. Alle Positionen bleiben gleich.
pub fn neutralize_attribute_macros(text: &str) -> String {
    let mut out = text.to_string();
    let bytes = text.as_bytes();
    for name in ATTRIBUTE_MACROS {
        let mut from = 0;
        while let Some(pos) = text[from..].find(name) {
            let start = from + pos;
            let end = start + name.len();
            let before_ok = start == 0 || !is_ident(bytes[start - 1]);
            let after_ok = end >= bytes.len() || !is_ident(bytes[end]);
            if before_ok && after_ok {
                out.replace_range(start..end, &" ".repeat(name.len()));
            }
            from = end;
        }
    }
    out
}

/// Parst C++ (`neutralize_attribute_macros` ist vorher anzuwenden, wenn Fehler zählen).
pub fn parse(text: &str) -> Tree {
    PARSER.with(|p| {
        p.borrow_mut()
            .parse(text, None)
            .expect("tree-sitter liefert immer einen Baum")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutralizing_keeps_length_and_whole_words_only() {
        let src = "void IRAM_ATTR isr() {}\nint MY_IRAM_ATTR_X = 1;";
        let out = neutralize_attribute_macros(src);
        assert_eq!(out.len(), src.len());
        assert!(out.starts_with("void           isr()"));
        assert!(out.contains("MY_IRAM_ATTR_X"));
    }
}
