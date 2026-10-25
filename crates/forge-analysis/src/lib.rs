//! Statische Analyse von Arduino-Sketches mit tree-sitter: Syntaxfehler, Funktionen, Includes,
//! Prototypen. Alles arbeitet auf dem Text aus dem Editor, ohne Datei und ohne Compiler.

mod analyze;
mod context;
mod functions;
mod includes;
mod parse;
mod prototypes;
mod syntax;

pub use analyze::{analyze, AnalysisInput, AnalysisResult, Chip, FunctionFacts, SourceFile};
pub use context::{blocks, Block};
pub use functions::{fingerprint, functions, FunctionInfo};
pub use includes::{includes, Include};
pub use parse::{neutralize_attribute_macros, parse};
pub use prototypes::{combine, Combined, SketchSource};
pub use syntax::syntax_diagnostics;
