use std::{fmt::Debug, path::PathBuf};

use knuffel::{
    span::{LinePos, LineSpan},
    traits::DecodeSpan,
};
use miette::SourceSpan;

#[derive(Clone)]
pub struct ParseSpan {
    pub start: LinePos,
    pub end: LinePos,
    pub file: PathBuf,
}

impl Debug for ParseSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ParseSpan")
            .field(&format!(
                "{}:{}-{}:{}",
                self.start.line, self.start.column, self.end.line, self.end.column
            ))
            .finish()
    }
}

pub struct FilePath(pub PathBuf);

impl DecodeSpan<LineSpan> for ParseSpan {
    fn decode_span(span: &LineSpan, ctx: &mut knuffel::decode::Context<LineSpan>) -> Self {
        Self {
            start: span.0,
            end: span.1,
            file: ctx.get::<FilePath>().unwrap().0.clone(),
        }
    }
}

impl ParseSpan {
    pub fn source_span(&self) -> SourceSpan {
        (self.start.offset, self.end.offset - self.start.offset).into()
    }
}
