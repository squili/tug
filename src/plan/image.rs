use futures_util::TryStreamExt;
use miette::{NamedSource, SourceSpan};
use podman_api::{
    opts::{ImageListFilter, ImageListOpts, PullOpts},
    Id,
};

use super::StepContext;
use crate::{parse::span::ParseSpan, utils::IntoDiagnosticShorthand};

#[derive(Clone, Debug)]
pub struct ImageAction {
    pub resolved: ResolvedImageRef,
    pub name: String,
    pub reference: String,
    pub reference_span: ParseSpan,
    pub local: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ResolvedImageRef(pub usize);

pub async fn execute(ctx: &StepContext, action: ImageAction) -> miette::Result<()> {
    let image_service = ctx.service.images();

    let (id, tag) = match action.reference.split_once(':') {
        Some((id, tag)) => (id, Some(tag.to_string())),
        None => (action.reference.as_str(), None),
    };

    let result = image_service
        .list(
            &ImageListOpts::builder()
                .filter([ImageListFilter::Reference(Id::from(id), tag)])
                .build(),
        )
        .await
        .d()?;

    if result.len() > 1 {
        println!(
            "Warning: two images found for reference {}, choosing the first one",
            action.reference
        );
    }

    if let Some(summary) = result.into_iter().next() {
        ctx.resolved_images.lock().insert(action.resolved, summary.id.unwrap());
        return Ok(());
    }

    if action.local {
        Err(ImageNotFound {
            name: action.reference.clone(),
            content: crate::prepare::diagnostics::read_source(&action.reference_span)?,
            reference: action.reference_span.source_span(),
            help: "images marked as local don't get automatically pulled",
        })?;
    }

    let mut stream = image_service.pull(&PullOpts::builder().reference(action.reference.clone()).build());

    while let Some(report) = stream.try_next().await.d()? {
        if let Some(err) = report.error {
            return Err(miette::miette!(err));
        }

        if let Some(id) = report.id {
            ctx.resolved_images.lock().insert(action.resolved, id);
            return Ok(());
        }
    }

    Err(miette::miette!("image stream completed without resolved id"))
}

#[derive(miette::Diagnostic, thiserror::Error, Debug)]
#[error("image `{name}` not found")]
struct ImageNotFound {
    name: String,
    #[source_code]
    content: NamedSource,
    #[label("referenced here")]
    reference: SourceSpan,
    #[help]
    help: &'static str,
}
