use knuffel::span::Spanned;
use miette::{NamedSource, SourceSpan};

use super::StepContext;
use crate::{
    parse::span::ParseSpan,
    utils::{IntoDiagnosticShorthand, XTug},
};

#[derive(Clone, Debug)]
pub struct SecretAction {
    pub resolved: ResolvedSecretRef,
    pub name: Spanned<String, ParseSpan>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ResolvedSecretRef(pub usize);

pub async fn execute(ctx: &StepContext, action: SecretAction) -> miette::Result<()> {
    let name = action.name.to_string();

    let secrets = ctx.service.secrets().list().await.d()?;
    let secrets = secrets
        .into_iter()
        .map(|report| {
            (
                report.id.unwrap_or_default(),
                report
                    .spec
                    .map(|spec| (spec.name.unwrap_or_default(), spec.labels.unwrap_or_default()))
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();

    if let Some((id, _)) = secrets.iter().find(|(_, secret)| {
        secret.1.get(XTug::Group.as_ref()) == Some(&ctx.group) && secret.1.get(XTug::Name.as_ref()) == Some(&name)
    }) {
        ctx.resolved_secrets.lock().insert(action.resolved, id.clone());
        return Ok(());
    }

    if let Some((id, _)) = secrets.iter().find(|(_, secret)| secret.0 == name) {
        ctx.resolved_secrets.lock().insert(action.resolved, id.clone());
        return Ok(());
    }

    Err(SecretNotFound {
        name,
        content: crate::prepare::diagnostics::read_source(action.name.span())?,
        reference: action.name.span().source_span(),
        help: "you can create secrets with `podman secret create`",
    })?
}

#[derive(miette::Diagnostic, thiserror::Error, Debug)]
#[error("secret `{name}` not found")]
struct SecretNotFound {
    name: String,
    #[source_code]
    content: NamedSource,
    #[label("referenced here")]
    reference: SourceSpan,
    #[help]
    help: &'static str,
}
