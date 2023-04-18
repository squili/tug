use knuffel::span::Spanned;
use miette::{Diagnostic, NamedSource, SourceSpan};

use crate::{parse::span::ParseSpan, utils::IntoDiagnosticShorthand};

pub fn read_source(span: &ParseSpan) -> miette::Result<NamedSource> {
    let content = std::fs::read_to_string(&span.file).d()?;
    Ok(NamedSource::new(span.file.to_string_lossy(), content))
}

#[derive(thiserror::Error, Debug, Diagnostic)]
#[error("duplicate name definition")]
pub struct DuplicateName {
    #[source_code]
    pub content: NamedSource,
    #[label("referenced here")]
    pub first_name: SourceSpan,
    #[label("original definition here")]
    pub second_name: Option<SourceSpan>,
}

impl DuplicateName {
    pub fn from_spans(first: &ParseSpan, second: &ParseSpan) -> miette::Result<()> {
        let content = std::fs::read_to_string(&second.file).d()?;
        Err(DuplicateName {
            content: NamedSource::new(second.file.to_string_lossy(), content),
            first_name: second.source_span(),
            second_name: (second.file == first.file).then(|| first.source_span()),
        })?
    }
}

#[derive(thiserror::Error, Debug, Diagnostic)]
#[error("unknown {thing}")]
pub struct UnknownThing {
    #[source_code]
    pub content: NamedSource,
    #[label("referenced here")]
    pub name: SourceSpan,
    pub thing: &'static str,
}

impl UnknownThing {
    pub fn new(name_space: Spanned<String, ParseSpan>, what: &'static str) -> miette::Result<()> {
        let span = name_space.span();
        let content = std::fs::read_to_string(&span.file).d()?;

        Err(UnknownThing {
            content: NamedSource::new(span.file.to_string_lossy(), content),
            name: span.source_span(),
            thing: what,
        })?
    }
}

#[derive(thiserror::Error, Debug, Diagnostic)]
#[error("duplicate inject paths")]
pub struct DuplicateInjectPath {
    #[source_code]
    pub content: NamedSource,
    #[label("first entry")]
    pub first: SourceSpan,
    #[label("second entry")]
    pub second: SourceSpan,
}

#[derive(thiserror::Error, Debug, Diagnostic)]
#[error("malformed command")]
pub struct MalformedCommand {
    #[source_code]
    pub content: NamedSource,
    #[label("defined here")]
    pub here: SourceSpan,
}
