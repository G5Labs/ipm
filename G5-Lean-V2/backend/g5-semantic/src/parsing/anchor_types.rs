use std::ops::Range;

/// A parsed I/D/Q block from a markdown file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedBlock {
    /// Label identifier: "I1", "D3", "Q5", etc.
    pub identifier: String,
    /// Block title text (the part after the number).
    pub title: String,
    /// Whether the block has human authority via the exact `(Tags: human)`
    /// semantic-record suffix (alone or before `uncertain`).
    pub is_human: bool,
    /// Whether the block carries canonical `uncertain` semantic metadata.
    pub is_uncertain: bool,
    /// Explicit priority metadata parsed from source markup, when supported.
    pub priority: Option<u8>,
    /// Explicit certainty metadata parsed from source markup, when supported.
    pub certainty: Option<u8>,
    /// 1-indexed line number where the label appears.
    pub line_number: usize,
    /// Byte range of the full block content in the source.
    pub byte_range: Range<usize>,
}

/// Result of parsing a markdown file for I/D/Q blocks.
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub blocks: Vec<ParsedBlock>,
    pub warnings: Vec<String>,
}

/// Content extracted from a specific block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedContent {
    pub content: String,
    pub boundary_uncertain: bool,
}

/// Error extracting a specific block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractError {
    NotFound,
    AmbiguousDuplicate(Vec<usize>),
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "Block not found"),
            Self::AmbiguousDuplicate(lines) => {
                write!(f, "Ambiguous duplicate block at lines: {:?}", lines)
            }
        }
    }
}

impl std::error::Error for ExtractError {}

/// A named code element extracted from a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCodeBlock {
    /// Element name, e.g. "TaskService::poll_next", "Task", "MAX_RETRY_COUNT".
    pub name: String,
    /// Kind of code element.
    pub kind: CodeElementKind,
    /// 1-indexed line number.
    pub line_number: usize,
    /// Byte range in the source.
    pub byte_range: Range<usize>,
}

/// Kind of code element extracted by the code parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeElementKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Const,
    TypeAlias,
    Class,
    Interface,
    Method,
    Namespace,
    Typedef,
}

impl CodeElementKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::Const => "const",
            Self::TypeAlias => "type_alias",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Method => "method",
            Self::Namespace => "namespace",
            Self::Typedef => "typedef",
        }
    }
}
