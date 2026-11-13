//! Parsing infrastructure for intent block extraction.
//!
//! - [`markdown_parser`]: Extracts I/D/Q-labeled blocks from markdown files
//!   using `pulldown-cmark`.
//! - [`code_parser`]: Extracts named code elements (functions, structs, etc.)
//!   from source files using `tree-sitter` with per-language profiles.
//! - [`anchor_types`]: Shared result types used by both parsers.

pub mod anchor_types;
pub mod code_parser;
pub mod language_profiles;
pub mod markdown_parser;

pub use anchor_types::*;
pub use code_parser::{extract_shell_blocks, CodeAnchorExtractor};
pub use markdown_parser::{block_line_range_by_label, MarkdownAnchorExtractor};
