use std::path::Path;

use walkdir::WalkDir;

use self::model::ParsedDocument;
use crate::{logger::Logger, parse::span::FilePath, utils::IntoDiagnosticShorthand};

pub mod model;
pub mod span;

pub fn parse(logger: &Logger, root: &Path) -> miette::Result<ParsedDocument> {
    logger.log("Parsing configuration documents");

    let mut merged = ParsedDocument::default();

    for ent in WalkDir::new(root).follow_links(true) {
        let ent = ent.d()?;
        let file_name = ent.file_name().to_str().expect("paths should be unicode");
        if !is_tugy(file_name) {
            continue;
        }
        let text = std::fs::read_to_string(ent.path()).d()?;
        let doc: ParsedDocument =
            knuffel::parse_with_context(file_name, &text, |ctx| ctx.set(FilePath(ent.path().to_path_buf())))?;
        merged.containers.extend(doc.containers);
        merged.images.extend(doc.images);
        merged.networks.extend(doc.networks);
        merged.volumes.extend(doc.volumes);
    }

    Ok(merged)
}

fn is_tugy(file_name: &str) -> bool {
    let mut iter = file_name.rsplit('.');
    match [iter.next(), iter.next()] {
        [Some(ext), Some(bit)] => ext == "kdl" && bit == "tug",
        [Some(_), _] => false,
        _ => unreachable!(),
    }
}
