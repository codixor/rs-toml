#![allow(clippy::question_mark)]

use crate::{Category, FormatError, Key, Position, Span};
use core::num::NonZeroU32;
use std::fmt::{self, Debug, Display};

#[derive(Clone, Copy)]
pub enum PathComponent<'de> {
    Key(Key<'de>),
    Index(usize),
}

impl PathComponent<'_> {
    pub(crate) fn redacted_key(key: Key<'_>) -> Self {
        Self::Key(Key {
            name: "<key>",
            span: key.span,
        })
    }
}

/// Structural path to an error. Key components retain their spans and use
/// the literal placeholder `<key>`; index components retain their indices.
#[repr(transparent)]
pub struct TomlPath<'a>([PathComponent<'a>]);

impl<'a> std::ops::Deref for TomlPath<'a> {
    type Target = [PathComponent<'a>];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Debug for TomlPath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        push_toml_path(&mut out, &self.0);
        Debug::fmt(&out, f)
    }
}

impl Display for TomlPath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        push_toml_path(&mut out, &self.0);
        f.write_str(&out)
    }
}

fn is_bare_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    for &b in key.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' => (),
            _ => return false,
        }
    }
    true
}

pub(crate) use crate::raw::path::MaybeTomlPath;

/// A single error from parsing or converting a TOML document.
///
/// Errors arise from two phases:
///
/// - Parsing: syntax errors such as unterminated strings, duplicate keys,
///   or unexpected characters. [`parse`](crate::parse) returns the first such
///   error as `Err(Error)`.
///
/// - Conversion: type mismatches, missing fields, unknown keys, and
///   other constraint violations detected by [`FromToml`](crate::FromToml).
///   These accumulate so that a single pass surfaces as many problems as
///   possible.
///
/// [`parse_recoverable`](crate::parse_recoverable) combines both phases,
/// continuing past syntax errors and collecting them alongside conversion
/// errors into a single [`Document::errors`](crate::Document::errors) list.
///
/// # Extracting information
///
/// | Method                                        | Returns                                                         |
/// |-----------------------------------------------|-----------------------------------------------------------------|
/// | [`kind()`](Self::kind)                        | The [`ErrorKind`] variant for this error                        |
/// | [`span()`](Self::span)                        | Source [`Span`] (byte offsets), possibly empty |
/// | [`path()`](Self::path)                        | Optional [`TomlPath`] to the offending value                    |
/// | [`message(source)`](Self::message)            | Human-readable diagnostic message                               |
/// | [`primary_label()`](Self::primary_label)      | Optional `(Span, String)` label for the error site              |
/// | [`secondary_label()`](Self::secondary_label)  | Optional `(Span, String)` for related locations                 |
///
/// The `message`, `primary_label`, and `secondary_label` methods provide
/// building blocks for rich diagnostics, mapping onto the label model used by
/// [`codespan-reporting`](https://docs.rs/codespan-reporting) and
/// [`annotate-snippets`](https://docs.rs/annotate-snippets).
///
/// # Integration with error reporting libraries
///
/// <details>
/// <summary><code>codespan-reporting</code> example</summary>
///
/// ```ignore
/// use codespan_reporting::diagnostic::{Diagnostic, Label};
///
/// fn error_to_diagnostic(
///     error: &rs_toml::Error,
///     source: &str,
/// ) -> Diagnostic<()> {
///     let mut labels = Vec::new();
///     if let Some((span, text)) = error.secondary_label() {
///         labels.push(Label::secondary((), span).with_message(text));
///     }
///     if let Some((span, label)) = error.primary_label() {
///         let l = Label::primary((), span);
///         labels.push(if label.is_empty() {
///             l
///         } else {
///             l.with_message(label)
///         });
///     }
///     Diagnostic::error()
///         .with_code(error.kind().kind_name())
///         .with_message(error.message_with_path(source))
///         .with_labels(labels)
/// }
/// ```
///
/// </details>
///
/// <details>
/// <summary><code>annotate-snippets</code> example</summary>
///
/// ```ignore
/// use annotate_snippets::{AnnotationKind, Group, Level, Snippet};
///
/// fn error_to_snippet<'s>(
///     error: &rs_toml::Error,
///     source: &'s str,
///     path: &'s str,
/// ) -> Group<'s> {
///     let message = error.message_with_path(source);
///     let mut snippet = Snippet::source(source).path(path).fold(true);
///     if let Some((span, text)) = error.secondary_label() {
///         snippet = snippet.annotation(
///             AnnotationKind::Context.span(span.range()).label(text),
///         );
///     }
///     if let Some((span, label)) = error.primary_label() {
///         let ann = AnnotationKind::Primary.span(span.range());
///         snippet = snippet.annotation(if label.is_empty() {
///             ann
///         } else {
///             ann.label(label)
///         });
///     }
///     Level::ERROR.primary_title(message).element(snippet)
/// }
/// ```
///
/// </details>
///
/// # Multiple error accumulation
///
/// During [`FromToml`](crate::FromToml) conversion, errors are pushed into a
/// shared [`Context`](crate::Context) rather than causing an immediate abort.
/// The sentinel type [`Failed`](crate::Failed) signals that a branch failed
/// without carrying error details itself. When
/// [`Document::to`](crate::Document::to) or [`from_str`](crate::from_str)
/// finishes, all accumulated errors are returned in
/// [`FromTomlError::errors`](crate::FromTomlError).
///
/// [`parse_recoverable`](crate::parse_recoverable) extends this to the
/// parsing phase, collecting syntax errors into the same list so valid
/// portions of the document remain available for inspection.
pub struct Error {
    pub(crate) kind: ErrorKind<'static>,
    pub(crate) category: Category,
    pub(crate) position: Option<Position>,
    pub(crate) span: Span,
    pub(crate) path: MaybeTomlPath,
}

/// The specific kind of error.
#[non_exhaustive]
#[derive(Clone, Copy)]
pub enum ErrorKind<'a> {
    /// A custom error message
    Custom(&'a str),

    /// EOF was reached when looking for a value.
    UnexpectedEof,

    /// The encoded document exceeds its configured or implementation limit.
    FileTooLarge,
    /// The encoded token body exceeds its configured limit.
    TokenLimit,
    /// The tree exceeds its configured or implementation depth.
    DepthLimit,

    /// An invalid character not allowed in a string was found.
    InvalidCharInString,

    /// An invalid character was found as an escape.
    InvalidEscape,

    /// An invalid character was found in a hex escape.
    InvalidHexEscape,

    /// An invalid escape value was specified in a hex escape in a string.
    ///
    /// Valid values are in the plane of unicode codepoints.
    InvalidEscapeValue,

    /// An unexpected character was encountered, typically when looking for a
    /// value.
    Unexpected,

    /// An unterminated string was found where EOF or a newline was reached
    /// before the closing delimiter.
    ///
    /// The `char` is the expected closing delimiter (`"` or `'`).
    UnterminatedString(char),

    /// The input array length differs from the required fixed length.
    InvalidLength,

    /// An integer literal failed to parse, with an optional reason.
    InvalidInteger(&'static str),

    /// A float literal failed to parse, with an optional reason.
    InvalidFloat(&'static str),

    /// A datetime literal failed to parse, with an optional reason.
    InvalidDateTime(&'static str),

    /// The number in the toml file cannot be losslessly converted to the specified
    /// number type
    OutOfRange {
        /// The target type name (e.g. `"u8"`)
        ty: &'static &'static str,
        /// The accepted range as a display string (e.g. `"0..=255"`), or empty
        range: &'static &'static str,
    },

    /// Wanted one sort of token, but found another.
    Wanted {
        /// Expected token type.
        expected: &'static &'static str,
        /// Actually found token type.
        found: &'static &'static str,
    },

    /// A duplicate table definition was found.
    DuplicateTable {
        /// The span of the table name (for extracting the name from source)
        name: Span,
        /// The span where the table was first defined
        first: Span,
    },

    /// Duplicate key in table.
    DuplicateKey {
        /// The span where the first key is located
        first: Span,
    },

    /// A previously defined table was redefined as an array.
    RedefineAsArray {
        /// The span where the table was first defined
        first: Span,
    },

    /// Multiline strings are not allowed for key.
    MultilineStringKey,

    /// Dotted key attempted to extend something that is not a table.
    DottedKeyInvalidType {
        /// The span where the non-table value was defined
        first: Span,
    },

    /// An unexpected key was encountered.
    ///
    /// Used when converting a struct with a limited set of fields.
    UnexpectedKey {
        /// Developer provided association tag useful for programmatic filtering
        /// or adding additional messages or notes to diagnostics. Defaults to 0.
        tag: u32,
    },

    /// Unquoted string was found when quoted one was expected.
    UnquotedString,

    /// A required field is missing from a table
    MissingField(&'static str),

    /// A field was set more than once (e.g. via primary key and alias)
    DuplicateField {
        /// The canonical struct field name
        field: &'static str,
        /// The span where the key was first defined
        first: Span,
    },

    /// A field in the table is deprecated and the new key should be used instead
    Deprecated {
        /// Developer provided association tag useful for programmatic filtering
        /// or adding additional messages or notes to diagnostics such as version
        /// info. Defaults to 0.
        tag: u32,
        /// The deprecated key name
        old: &'static &'static str,
        /// The key name that should be used instead
        new: &'static &'static str,
    },

    /// An unexpected value was encountered
    UnexpectedValue {
        /// The list of values that could have been used
        expected: &'static [&'static str],
    },

    /// A string did not match any known variant
    UnexpectedVariant {
        /// The list of variant names that would have been accepted
        expected: &'static [&'static str],
    },

    /// A comma is missing between elements in an array.
    MissingArrayComma,

    /// An array was not closed before EOF.
    UnclosedArray,

    /// A comma is missing between entries in an inline table.
    MissingInlineTableComma,

    /// An inline table was not closed before EOF or a newline.
    UnclosedInlineTable,
}

impl<'a> ErrorKind<'a> {
    pub fn kind_name(&self) -> &'static str {
        match self {
            ErrorKind::Custom(_) => "Custom",
            ErrorKind::UnexpectedEof => "UnexpectedEof",
            ErrorKind::FileTooLarge => "FileTooLarge",
            ErrorKind::TokenLimit => "TokenLimit",
            ErrorKind::DepthLimit => "DepthLimit",
            ErrorKind::InvalidCharInString => "InvalidCharInString",
            ErrorKind::InvalidEscape => "InvalidEscape",
            ErrorKind::InvalidHexEscape => "InvalidHexEscape",
            ErrorKind::InvalidEscapeValue => "InvalidEscapeValue",
            ErrorKind::Unexpected => "Unexpected",
            ErrorKind::UnterminatedString(_) => "UnterminatedString",
            ErrorKind::InvalidLength => "InvalidLength",
            ErrorKind::InvalidInteger(_) => "InvalidInteger",
            ErrorKind::InvalidFloat(_) => "InvalidFloat",
            ErrorKind::InvalidDateTime(_) => "InvalidDateTime",
            ErrorKind::OutOfRange { .. } => "OutOfRange",
            ErrorKind::Wanted { .. } => "Wanted",
            ErrorKind::DuplicateTable { .. } => "DuplicateTable",
            ErrorKind::DuplicateKey { .. } => "DuplicateKey",
            ErrorKind::RedefineAsArray { .. } => "RedefineAsArray",
            ErrorKind::MultilineStringKey => "MultilineStringKey",
            ErrorKind::DottedKeyInvalidType { .. } => "DottedKeyInvalidType",
            ErrorKind::UnexpectedKey { .. } => "UnexpectedKey",
            ErrorKind::UnquotedString => "UnquotedString",
            ErrorKind::MissingField(_) => "MissingField",
            ErrorKind::DuplicateField { .. } => "DuplicateField",
            ErrorKind::Deprecated { .. } => "Deprecated",
            ErrorKind::UnexpectedValue { .. } => "UnexpectedValue",
            ErrorKind::UnexpectedVariant { .. } => "UnexpectedVariant",
            ErrorKind::MissingArrayComma => "MissingArrayComma",
            ErrorKind::UnclosedArray => "UnclosedArray",
            ErrorKind::MissingInlineTableComma => "MissingInlineTableComma",
            ErrorKind::UnclosedInlineTable => "UnclosedInlineTable",
        }
    }
}

impl Error {
    /// Returns the source span where this error occurred.
    ///
    /// An empty span can mark an insertion point or EOF. Use
    /// [`FormatError::position`] to distinguish a known offset from no location.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Returns the error kind.
    pub fn kind(&self) -> ErrorKind<'_> {
        self.kind
    }

    /// Returns the TOML path where this error occurred, if available.
    pub fn path<'a>(&'a self) -> Option<&'a TomlPath<'a>> {
        self.path.as_toml_path()
    }

    /// Creates a data error at the given span. Free-form messages are discarded.
    pub fn custom(_message: impl ToString, span: Span) -> Error {
        Self::new(ErrorKind::Custom("conversion failed"), span)
    }

    pub(crate) fn custom_static(_message: &'static str, span: Span) -> Error {
        Self::new(ErrorKind::Custom("conversion failed"), span)
    }

    pub(crate) fn new(kind: ErrorKind<'static>, span: Span) -> Error {
        Self::new_with_path(kind, span, MaybeTomlPath::empty())
    }

    pub(crate) fn new_with_path(kind: ErrorKind<'static>, span: Span, path: MaybeTomlPath) -> Self {
        let position = (!span.is_empty()).then(|| offset_position(span.start));
        Self {
            kind,
            span,
            path,
            position,
            category: Category::Data,
        }
    }

    pub(crate) fn at_parser_position(mut self, source: &str, location: &mut Location) -> Self {
        self.category = match self.kind {
            ErrorKind::FileTooLarge | ErrorKind::DepthLimit | ErrorKind::TokenLimit => {
                Category::Limit
            }
            ErrorKind::UnexpectedEof => Category::Eof,
            ErrorKind::UnterminatedString(_)
            | ErrorKind::UnclosedArray
            | ErrorKind::UnclosedInlineTable
                if self.span.start as usize == source.len() =>
            {
                Category::Eof
            }
            ErrorKind::Wanted { found: &"eof", .. } => Category::Eof,
            _ => Category::Syntax,
        };
        self.position = location.advance(source, self.span.start);
        self
    }
}

impl FormatError for Error {
    fn category(&self) -> Category {
        self.category
    }
    fn position(&self) -> Option<Position> {
        self.position
    }
}

pub(crate) fn offset_position(offset: u32) -> Position {
    Position {
        offset: Some(u64::from(offset)),
        line: None,
        column: None,
    }
}

#[derive(Default)]
pub(crate) struct Location {
    offset: u32,
    newlines: u32,
    line_start: u32,
}

impl Location {
    pub(crate) fn advance(&mut self, source: &str, offset: u32) -> Option<Position> {
        if offset < self.offset {
            *self = Self::default();
        }
        let segment = source
            .as_bytes()
            .get(self.offset as usize..offset as usize)?;
        for (i, &byte) in segment.iter().enumerate() {
            if byte == b'\n' {
                self.newlines += 1;
                self.line_start = self.offset + i as u32 + 1;
            }
        }
        self.offset = offset;
        Some(Position {
            offset: Some(u64::from(offset)),
            line: NonZeroU32::new(self.newlines + 1),
            column: NonZeroU32::new(offset - self.line_start + 1),
        })
    }
}

// Typed conversion may report many errors in arbitrary field order. Locate them
// in one forward scan without building an index for every line of the document.
pub(crate) fn locate_errors(source: &str, errors: &mut [Error]) {
    let mut pending: Vec<_> = errors
        .iter_mut()
        .filter(|e| e.category == Category::Data)
        .collect();
    pending.sort_unstable_by_key(|e| e.span.start);
    let mut location = Location::default();
    for error in pending {
        error.position = location.advance(source, error.span.start);
    }
}

impl<'a> Debug for ErrorKind<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind_name())
    }
}

#[inline(never)]
fn s_push(out: &mut String, s: &str) {
    out.push_str(s);
}

#[inline(never)]
fn s_push_char(out: &mut String, c: char) {
    out.push(c);
}

fn push_u32(out: &mut String, mut n: u32) {
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    if n == 0 {
        s_push_char(out, '0');
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &digit in &buf[i..] {
        s_push_char(out, char::from(digit));
    }
}

fn kind_message(kind: ErrorKind<'_>) -> &'static str {
    match kind {
        ErrorKind::Custom(_) => "conversion failed",
        ErrorKind::UnexpectedEof => "unexpected end of input",
        ErrorKind::FileTooLarge => "document byte limit exceeded",
        ErrorKind::DepthLimit => "Max recursion depth exceeded",
        ErrorKind::TokenLimit => "token byte limit exceeded",
        ErrorKind::InvalidCharInString => "invalid character in string",
        ErrorKind::InvalidEscape => "invalid escape in string",
        ErrorKind::InvalidHexEscape => "invalid hex escape",
        ErrorKind::InvalidEscapeValue => "invalid Unicode escape",
        ErrorKind::Unexpected => "unexpected character",
        ErrorKind::UnterminatedString(_) => "unterminated string",
        ErrorKind::InvalidLength => "invalid array length",
        ErrorKind::InvalidInteger(_) => "invalid integer",
        ErrorKind::InvalidFloat(_) => "invalid float",
        ErrorKind::InvalidDateTime(_) => "invalid datetime",
        ErrorKind::OutOfRange { .. } => "value out of range",
        ErrorKind::Wanted { .. } => "unexpected type or token",
        ErrorKind::DuplicateTable { .. } => "redefinition of table",
        ErrorKind::DuplicateKey { .. } | ErrorKind::DuplicateField { .. } => "duplicate key",
        ErrorKind::RedefineAsArray { .. } => "table redefined as array",
        ErrorKind::MultilineStringKey => "multiline keys are not allowed",
        ErrorKind::DottedKeyInvalidType { .. } => "dotted key extends a non-table value",
        ErrorKind::UnexpectedKey { .. } => "unexpected key",
        ErrorKind::UnquotedString => "string values must be quoted",
        ErrorKind::MissingField(_) => "missing required key",
        ErrorKind::Deprecated { .. } => "deprecated key",
        ErrorKind::UnexpectedValue { .. } => "unexpected value",
        ErrorKind::UnexpectedVariant { .. } => "unknown variant",
        ErrorKind::MissingArrayComma => "missing comma between array elements",
        ErrorKind::UnclosedArray => "unclosed array",
        ErrorKind::MissingInlineTableComma => "missing comma between inline table fields",
        ErrorKind::UnclosedInlineTable => "unclosed inline table",
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(kind_message(self.kind))
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("kind", &self.kind)
            .field("category", &self.category)
            .field("span", &self.span.range())
            .field("position", &self.position)
            .finish()
    }
}

impl std::error::Error for Error {}

impl From<(ErrorKind<'static>, Span)> for Error {
    fn from((kind, span): (ErrorKind<'static>, Span)) -> Self {
        Self::new(kind, span)
    }
}

fn push_toml_path(out: &mut String, path: &[PathComponent<'_>]) {
    let mut first = true;
    for component in path.iter() {
        match component {
            PathComponent::Key(key) => {
                if !first {
                    s_push_char(out, '.');
                }
                first = false;
                if is_bare_key(key.name) {
                    s_push(out, key.name);
                } else {
                    s_push_char(out, '"');
                    for ch in key.name.chars() {
                        match ch {
                            '"' => s_push(out, "\\\""),
                            '\\' => s_push(out, "\\\\"),
                            '\n' => s_push(out, "\\n"),
                            '\r' => s_push(out, "\\r"),
                            '\t' => s_push(out, "\\t"),
                            c if c.is_control() => {
                                push_unicode_escape(out, c as u32);
                            }
                            c => s_push_char(out, c),
                        }
                    }
                    s_push_char(out, '"');
                }
            }
            PathComponent::Index(idx) => {
                s_push_char(out, '[');
                push_u32(out, *idx as u32);
                s_push_char(out, ']');
            }
        }
    }
}

fn push_unicode_escape(out: &mut String, n: u32) {
    s_push(out, "\\u");
    let mut buf = [0u8; 8];
    let digits = if n <= 0xFFFF { 4 } else { 6 };
    for i in (0..digits).rev() {
        let nibble = ((n >> (i * 4)) & 0xF) as u8;
        buf[digits - 1 - i] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'A' + nibble - 10
        };
    }
    for &digit in &buf[..digits] {
        s_push_char(out, char::from(digit));
    }
}

impl Error {
    /// Returns the diagnostic description without quoting input.
    pub fn message(&self, _source: &str) -> String {
        String::from(kind_message(self.kind))
    }

    /// Returns the redacted diagnostic description. Use `span` for the location.
    pub fn message_with_path(&self, source: &str) -> String {
        self.message(source)
    }

    /// Returns the source span and its redacted diagnostic label.
    pub fn primary_label(&self) -> Option<(Span, String)> {
        Some((self.span, String::from(kind_message(self.kind))))
    }

    /// Returns the secondary label span and text, if any.
    ///
    /// Some errors reference a related location (for example, the first
    /// definition of a duplicate key).
    pub fn secondary_label(&self) -> Option<(Span, String)> {
        let (first, text) = match self.kind() {
            ErrorKind::DuplicateKey { first } => (first, "first key instance"),
            ErrorKind::DuplicateTable { first, .. } => (first, "first table instance"),
            ErrorKind::DottedKeyInvalidType { first } => (first, "non-table"),
            ErrorKind::RedefineAsArray { first } => (first, "first defined as table"),
            ErrorKind::DuplicateField { first, .. } => (first, "first defined here"),
            _ => return None,
        };
        Some((first, String::from(text)))
    }
}
