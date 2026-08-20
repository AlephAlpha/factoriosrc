//! Exporting search results to RLE files.
//!
//! The frontends can use [`Template`] to turn a user-specified file-name
//! template into a file name for each result, and [`save_generation`] to
//! write one generation of a solution to a file.

use crate::{Config, Symmetry, Transformation};
use std::path::PathBuf;

/// The default file-name template for exported results.
///
/// Used by the frontends when the user enables result export without
/// providing a template.
pub const DEFAULT_EXPORT_TEMPLATE: &str =
    "{rule}_{width}x{height}_{dx},{dy}_{symmetry}_{index:04}.rle";

/// A file-name-safe token for a [`Symmetry`].
///
/// The display strings of `Symmetry` may contain characters that are not
/// allowed in file names, such as `|`, `\`, `/`, and `+`. The tokens returned
/// by this function only contain ASCII letters and digits, and are unique for
/// every symmetry.
#[inline]
#[must_use]
pub const fn symmetry_token(symmetry: Symmetry) -> &'static str {
    match symmetry {
        Symmetry::C1 => "C1",
        Symmetry::C2 => "C2",
        Symmetry::C4 => "C4",
        Symmetry::D2H => "D2H",
        Symmetry::D2V => "D2V",
        Symmetry::D2D => "D2D",
        Symmetry::D2A => "D2A",
        Symmetry::D4O => "D4O",
        Symmetry::D4X => "D4X",
        Symmetry::D8 => "D8",
    }
}

/// Replace the characters that are not allowed in file names.
///
/// The characters `<`, `>`, `:`, `"`, `/`, `\`, `|`, `?`, and `*` are
/// replaced by `_`, together with control characters. Trailing dots and
/// spaces are removed, since they are not allowed at the end of a file name
/// on Windows. If the result is empty, a single `_` is returned.
#[must_use]
pub fn sanitize_component(s: &str) -> String {
    let mut result: String = s
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    while let Some(c) = result.chars().last()
        && (c == '.' || c == ' ')
    {
        result.pop();
    }
    if result.is_empty() {
        result.push('_');
    }
    result
}

/// The placeholders that can appear in an export template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placeholder {
    /// The rule string.
    Rule,
    /// The width of the search world.
    Width,
    /// The height of the search world.
    Height,
    /// The period of the search.
    Period,
    /// The horizontal translation.
    Dx,
    /// The vertical translation.
    Dy,
    /// The symmetry of the search.
    Symmetry,
    /// The transformation of the search.
    Transformation,
    /// The 1-based index of the solution.
    Index,
    /// The 0-based index of the generation.
    Generation,
    /// The population of the generation.
    Population,
}

impl Placeholder {
    /// Parse a placeholder from its name.
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "rule" => Self::Rule,
            "width" => Self::Width,
            "height" => Self::Height,
            "period" => Self::Period,
            "dx" => Self::Dx,
            "dy" => Self::Dy,
            "symmetry" => Self::Symmetry,
            "transformation" => Self::Transformation,
            "index" => Self::Index,
            "generation" => Self::Generation,
            "population" => Self::Population,
            _ => return None,
        })
    }

    /// The name of the placeholder.
    const fn name(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Width => "width",
            Self::Height => "height",
            Self::Period => "period",
            Self::Dx => "dx",
            Self::Dy => "dy",
            Self::Symmetry => "symmetry",
            Self::Transformation => "transformation",
            Self::Index => "index",
            Self::Generation => "generation",
            Self::Population => "population",
        }
    }

    /// Whether the value is a string that may need to be sanitized.
    const fn is_string_field(self) -> bool {
        matches!(self, Self::Rule | Self::Symmetry | Self::Transformation)
    }
}

/// The format spec of a placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormatSpec {
    /// No spec.
    None,
    /// The `raw` spec: do not sanitize string fields.
    Raw,
    /// The `0N` spec: zero-pad numeric fields to a width of `N`.
    Pad(usize),
}

impl FormatSpec {
    /// Parse a format spec for a placeholder.
    fn parse(spec: &str, placeholder: Placeholder) -> Result<Self, TemplateError> {
        if spec == "raw" {
            return Ok(Self::Raw);
        }
        if let Some(width) = spec
            .strip_prefix('0')
            .and_then(|rest| rest.parse::<usize>().ok())
            && width > 0
        {
            if placeholder.is_string_field() {
                return Err(TemplateError::InvalidSpec {
                    name: placeholder.name().to_string(),
                    spec: spec.to_string(),
                });
            }
            return Ok(Self::Pad(width));
        }
        Err(TemplateError::InvalidSpec {
            name: placeholder.name().to_string(),
            spec: spec.to_string(),
        })
    }
}

/// A segment of a parsed [`Template`].
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    /// A literal part of the template.
    Literal(String),
    /// A placeholder with its format spec.
    Field {
        /// The placeholder.
        placeholder: Placeholder,
        /// The format spec.
        spec: FormatSpec,
    },
}

/// An error that occurs when parsing an export template.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    /// A `{` is not closed by a `}`.
    #[error("unclosed `{{` in export template")]
    UnclosedBrace,
    /// A placeholder has no name.
    #[error("empty placeholder in export template")]
    EmptyPlaceholder,
    /// The placeholder name is not recognized.
    #[error("unknown placeholder `{name}` in export template")]
    UnknownPlaceholder {
        /// The name of the unknown placeholder.
        name: String,
    },
    /// The format spec is not valid for the placeholder.
    #[error(
        "invalid format spec `{spec}` for placeholder `{name}` in export template \
         (expected `raw` or `0N` with N > 0)"
    )]
    InvalidSpec {
        /// The name of the placeholder.
        name: String,
        /// The invalid spec.
        spec: String,
    },
}

/// A parsed export file-name template.
///
/// A template is a file name that may contain `{placeholder}` fields. The
/// supported placeholders are `rule`, `width`, `height`, `period`, `dx`,
/// `dy`, `symmetry`, `transformation`, `index`, `generation`, and
/// `population`. A placeholder may be followed by a format spec:
///
/// - `raw`: do not sanitize the value of a string field.
/// - `0N`: zero-pad a numeric field to a width of `N`, e.g. `{index:04}`.
///
/// The string fields `rule`, `symmetry`, and `transformation` are sanitized
/// by default, so that the generated file name does not contain characters
/// that are not allowed in file names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// The segments of the template.
    segments: Vec<Segment>,
    /// Whether the template contains the `generation` placeholder.
    has_generation: bool,
}

impl Template {
    /// Parse a template from a string.
    ///
    /// # Errors
    ///
    /// Returns a [`TemplateError`] if the template contains an unclosed `{`,
    /// an empty placeholder, an unknown placeholder name, or an invalid
    /// format spec.
    pub fn parse(s: &str) -> Result<Self, TemplateError> {
        let mut segments = Vec::new();
        let mut literal = String::new();
        let mut chars = s.chars();
        let mut has_generation = false;

        while let Some(c) = chars.next() {
            if c != '{' {
                literal.push(c);
                continue;
            }

            let mut name = String::new();
            let mut spec = None;
            let mut closed = false;
            for c in chars.by_ref() {
                if c == '}' {
                    closed = true;
                    break;
                }
                if c == ':' {
                    let mut s = String::new();
                    for c in chars.by_ref() {
                        if c == '}' {
                            closed = true;
                            break;
                        }
                        s.push(c);
                    }
                    spec = Some(s);
                    break;
                }
                name.push(c);
            }
            if !closed {
                return Err(TemplateError::UnclosedBrace);
            }
            if name.is_empty() {
                return Err(TemplateError::EmptyPlaceholder);
            }
            let placeholder = Placeholder::parse(&name)
                .ok_or_else(|| TemplateError::UnknownPlaceholder { name: name.clone() })?;
            let spec = match spec {
                Some(spec) => FormatSpec::parse(&spec, placeholder)?,
                None => FormatSpec::None,
            };
            has_generation |= placeholder == Placeholder::Generation;
            if !literal.is_empty() {
                segments.push(Segment::Literal(std::mem::take(&mut literal)));
            }
            segments.push(Segment::Field { placeholder, spec });
        }

        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }

        Ok(Self {
            segments,
            has_generation,
        })
    }

    /// Whether the template contains the `generation` placeholder.
    pub const fn has_generation(&self) -> bool {
        self.has_generation
    }

    /// Expand the template with the given fields.
    #[must_use]
    pub fn expand(&self, fields: &ExportFields) -> String {
        let mut result = String::new();
        for segment in &self.segments {
            match segment {
                Segment::Literal(s) => result.push_str(s),
                Segment::Field { placeholder, spec } => match placeholder {
                    Placeholder::Rule => {
                        result.push_str(&match spec {
                            FormatSpec::Raw => fields.rule.clone(),
                            _ => sanitize_component(&fields.rule),
                        });
                    }
                    Placeholder::Symmetry => {
                        let raw = match spec {
                            FormatSpec::Raw => fields.symmetry.to_string(),
                            _ => symmetry_token(fields.symmetry).to_string(),
                        };
                        result.push_str(&raw);
                    }
                    Placeholder::Transformation => {
                        let raw = match spec {
                            FormatSpec::Raw => fields.transformation.to_string(),
                            _ => sanitize_component(&fields.transformation.to_string()),
                        };
                        result.push_str(&raw);
                    }
                    Placeholder::Width => result.push_str(&format_u32(spec, fields.width)),
                    Placeholder::Height => {
                        result.push_str(&format_u32(spec, fields.height));
                    }
                    Placeholder::Period => {
                        result.push_str(&format_u32(spec, fields.period));
                    }
                    Placeholder::Dx => result.push_str(&format_i32(spec, fields.dx)),
                    Placeholder::Dy => result.push_str(&format_i32(spec, fields.dy)),
                    Placeholder::Index => result.push_str(&format_usize(spec, fields.index)),
                    Placeholder::Generation => {
                        result.push_str(&format_u32(spec, fields.generation));
                    }
                    Placeholder::Population => {
                        result.push_str(&format_usize(spec, fields.population));
                    }
                },
            }
        }
        result
    }
}

/// Format an unsigned integer with the given spec.
fn format_u32(spec: &FormatSpec, value: u32) -> String {
    match spec {
        FormatSpec::Pad(width) => format!("{value:0width$}"),
        _ => value.to_string(),
    }
}

/// Format an unsigned integer with the given spec.
fn format_usize(spec: &FormatSpec, value: usize) -> String {
    match spec {
        FormatSpec::Pad(width) => format!("{value:0width$}"),
        _ => value.to_string(),
    }
}

/// Format a signed integer with the given spec.
fn format_i32(spec: &FormatSpec, value: i32) -> String {
    match spec {
        FormatSpec::Pad(width) => format!("{value:0width$}"),
        _ => value.to_string(),
    }
}

/// The values that can be substituted into an export template.
#[derive(Debug, Clone)]
pub struct ExportFields {
    /// The raw rule string.
    pub rule: String,
    /// The width of the search world.
    pub width: u32,
    /// The height of the search world.
    pub height: u32,
    /// The period of the search.
    pub period: u32,
    /// The horizontal translation.
    pub dx: i32,
    /// The vertical translation.
    pub dy: i32,
    /// The symmetry of the search.
    pub symmetry: Symmetry,
    /// The transformation of the search.
    pub transformation: Transformation,
    /// The 1-based index of the solution.
    pub index: usize,
    /// The 0-based index of the generation.
    pub generation: u32,
    /// The population of the generation.
    pub population: usize,
}

impl ExportFields {
    /// Create the fields from a [`Config`], the 1-based solution `index`, and
    /// the 0-based `generation` with its `population`.
    #[must_use]
    pub fn from_config(config: &Config, index: usize, generation: u32, population: usize) -> Self {
        Self {
            rule: config.rule_str.clone(),
            width: config.width,
            height: config.height,
            period: config.period,
            dx: config.dx,
            dy: config.dy,
            symmetry: config.symmetry,
            transformation: config.transformation,
            index,
            generation,
            population,
        }
    }
}

/// An error that occurs when exporting a result to a file.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// The export template is invalid.
    #[error("invalid export template: {0}")]
    Template(#[from] TemplateError),
    /// A parent directory could not be created.
    #[error("failed to create directory `{path}`: {source}")]
    CreateDir {
        /// The directory that could not be created.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The file could not be written.
    #[error("failed to write `{path}`: {source}")]
    Write {
        /// The file that could not be written.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
}

/// Write one generation of a solution to a file.
///
/// The `template` is expanded with the `fields` to get the file name. Parent
/// directories are created if necessary. The `.rle` extension is appended if
/// the expanded name does not already end with it. If the period is greater
/// than 1 and the template does not contain the `generation` placeholder, a
/// `_g<generation>` suffix is inserted before the extension, so that the
/// generations of a solution do not overwrite each other.
///
/// This function is not available on `wasm32`, since `std::fs` is not
/// available there.
///
/// # Errors
///
/// Returns an [`ExportError`] if the file cannot be created or written.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_generation(
    template: &Template,
    fields: &ExportFields,
    rle: &str,
) -> Result<PathBuf, ExportError> {
    let name = template.expand(fields);
    let (stem, extension) = if name.to_lowercase().ends_with(".rle") {
        name.split_at(name.len() - 4)
    } else {
        (name.as_str(), "")
    };
    let mut name = stem.to_string();
    if fields.period > 1 && !template.has_generation() {
        name.push_str(&format!("_g{}", fields.generation));
    }
    if extension.is_empty() {
        name.push_str(".rle");
    } else {
        name.push_str(extension);
    }
    let path = PathBuf::from(&name);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|source| ExportError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(&path, rle).map_err(|source| ExportError::Write {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    // Export templates use `{name:spec}` syntax, which looks like formatting
    // arguments to clippy.
    #![allow(clippy::literal_string_with_formatting_args)]

    use super::*;
    use crate::Config;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn sanitize_replaces_unsafe_characters() {
        assert_eq!(sanitize_component("B3/S23"), "B3_S23");
        assert_eq!(sanitize_component("D2/"), "D2_");
        assert_eq!(sanitize_component("D2\\"), "D2_");
        assert_eq!(sanitize_component("D2|"), "D2_");
        assert_eq!(sanitize_component("R3,C2,S2,B3,N+"), "R3,C2,S2,B3,N+");
        assert_eq!(sanitize_component("ends."), "ends");
        assert_eq!(sanitize_component("a b "), "a b");
        assert_eq!(sanitize_component("with\0nul"), "with_nul");
        assert_eq!(sanitize_component("//"), "__");
        assert_eq!(sanitize_component("::"), "__");
    }

    #[test]
    fn symmetry_tokens_are_unique() {
        use Symmetry::*;
        let tokens = [C1, C2, C4, D2H, D2V, D2D, D2A, D4O, D4X, D8].map(symmetry_token);
        let unique: std::collections::HashSet<&str> = tokens.into_iter().collect();
        assert_eq!(unique.len(), tokens.len());
        assert_eq!(symmetry_token(D2H), "D2H");
        assert_eq!(symmetry_token(D2A), "D2A");
        assert_eq!(symmetry_token(D4O), "D4O");
    }

    fn fields(config: &Config, index: usize, generation: u32) -> ExportFields {
        ExportFields::from_config(config, index, generation, 42)
    }

    fn sample_config() -> Config {
        let mut config = Config::new("B3/S23", 20, 10, 2);
        config.dx = -1;
        config.dy = 0;
        config.symmetry = Symmetry::D2A;
        config.transformation = Transformation::R0;
        config
    }

    #[test]
    fn expand_substitutes_fields() {
        let template = Template::parse("{rule}_{width}x{height}_{dx},{dy}_{symmetry}").unwrap();
        let config = sample_config();
        assert_eq!(
            template.expand(&fields(&config, 3, 1)),
            "B3_S23_20x10_-1,0_D2A"
        );
    }

    #[test]
    fn expand_zero_pads_numeric_fields() {
        let template = Template::parse(r"{index:04}_{generation:02}").unwrap();
        let config = sample_config();
        assert_eq!(template.expand(&fields(&config, 7, 3)), "0007_03");
        assert!(template.has_generation());
    }

    #[test]
    fn expand_raw_spec_keeps_raw_values() {
        let template = Template::parse(r"{rule:raw}_{symmetry:raw}").unwrap();
        let config = sample_config();
        assert_eq!(template.expand(&fields(&config, 1, 0)), "B3/S23_D2/");
    }

    #[test]
    fn expand_default_template() {
        let template = Template::parse(DEFAULT_EXPORT_TEMPLATE).unwrap();
        let config = sample_config();
        assert_eq!(
            template.expand(&fields(&config, 4, 0)),
            "B3_S23_20x10_-1,0_D2A_0004.rle"
        );
        assert!(!template.has_generation());
    }

    #[test]
    fn parse_rejects_unknown_placeholder() {
        assert!(matches!(
            Template::parse("{foo}"),
            Err(TemplateError::UnknownPlaceholder { .. })
        ));
    }

    #[test]
    fn parse_rejects_unclosed_brace() {
        assert!(matches!(
            Template::parse("foo{bar"),
            Err(TemplateError::UnclosedBrace)
        ));
    }

    #[test]
    fn parse_rejects_empty_placeholder() {
        assert!(matches!(
            Template::parse("{}"),
            Err(TemplateError::EmptyPlaceholder)
        ));
    }

    #[test]
    fn parse_rejects_padding_on_string_fields() {
        assert!(matches!(
            Template::parse("{rule:04}"),
            Err(TemplateError::InvalidSpec { .. })
        ));
    }

    #[test]
    fn parse_rejects_invalid_spec() {
        assert!(matches!(
            Template::parse("{index:4}"),
            Err(TemplateError::InvalidSpec { .. })
        ));
    }

    fn unique_temp_dir() -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "factoriosrc-export-test-{}-{n}",
            std::process::id()
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn save_generation_writes_files() {
        let config = sample_config();
        let dir = unique_temp_dir();
        let template =
            Template::parse(&format!("{}/{{rule}}_{{index:03}}", dir.display())).unwrap();
        let path = save_generation(
            &template,
            &fields(&config, 5, 0),
            "x = 20, y = 10, rule = B3/S23\nbo!\n",
        )
        .unwrap();

        // The period is 2, and the template has no `generation` placeholder,
        // so a `_g0` suffix is appended.
        assert_eq!(path, dir.join("B3_S23_005_g0.rle"));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "x = 20, y = 10, rule = B3/S23\nbo!\n"
        );

        let path2 = save_generation(
            &template,
            &fields(&config, 5, 1),
            "x = 20, y = 10, rule = B3/S23\nob!\n",
        )
        .unwrap();
        assert_eq!(path2, dir.join("B3_S23_005_g1.rle"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn save_generation_creates_parent_directories() {
        let config = sample_config();
        let dir = unique_temp_dir();
        let template = Template::parse(&format!("{}/a/b/{{index}}.rle", dir.display())).unwrap();
        let path = save_generation(&template, &fields(&config, 1, 0), "x = 1, y = 1!\n").unwrap();
        assert_eq!(path, dir.join("a/b/1_g0.rle"));
        assert!(path.is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
