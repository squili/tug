use std::collections::BTreeSet;

use clap::Parser;
use futures_util::TryStreamExt;
use podman_api::{
    opts::{ImageExportOpts, ImageListFilter, ImageListOpts, ImageTagOpts},
    Podman,
};

use crate::{config::Config, logger::Logger, utils::IntoDiagnosticShorthand};

#[derive(Parser)]
pub struct Args {
    #[arg(short, long)]
    tag: String,
    #[arg(short, long)]
    local: String,
    #[arg(short, long)]
    select: bool,
}

impl Args {
    pub async fn execute(self, config: Config, logger: Logger) -> miette::Result<()> {
        let remote = config.service(&logger, false).await?;
        let local = Podman::new(&self.local).d()?;

        let (name, tag) = match self.tag.rsplit_once(':') {
            Some((name, tag)) => (name, Some(tag.to_string())),
            None => (self.tag.as_str(), None),
        };

        let mut images = local
            .images()
            .list(
                &ImageListOpts::builder()
                    .filter([ImageListFilter::Reference(name.into(), tag)])
                    .build(),
            )
            .await
            .d()?;

        if self.select {
            match dialoguer::MultiSelect::new()
                .items(
                    &images
                        .iter()
                        .map(|image| {
                            let names = image.names.clone().unwrap_or_default().join(", ");
                            let mut id = image.id.clone().unwrap_or_default();
                            id.truncate(12);
                            format!("{names} ({id})")
                        })
                        .collect::<Vec<_>>(),
                )
                .interact_opt()
                .d()?
            {
                Some(selections) => {
                    let selections = BTreeSet::from_iter(selections);
                    images = images
                        .into_iter()
                        .enumerate()
                        .filter(|(index, _)| selections.contains(index))
                        .map(|(_, image)| image)
                        .collect::<Vec<_>>();
                }
                None => {
                    logger.log("Cancelled");
                    return Ok(());
                }
            }
        }

        let local_images = local.images();

        for image in images {
            let id = image.id.unwrap_or_default();
            let id_short = &id.as_str()[..12];
            logger.log(format!("Processing {id_short}"));

            logger.log("Exporting");
            let image_object = local_images.get(&id);
            let mut export_stream = image_object.export(&ImageExportOpts::builder().compress(true).build());
            let mut buffer = Vec::new();
            while let Some(chunk) = export_stream.try_next().await.d()? {
                buffer.extend(chunk);
            }

            logger.log("Importing");
            remote.images().load(&buffer).await.d()?;
            let remote_image = remote.images().get(id);
            for repo_tag in image.repo_tags.unwrap_or_default() {
                let (repo, tag) = match repo_tag.rsplit_once(':') {
                    Some((repo, tag)) => (repo, Some(tag)),
                    None => (repo_tag.as_str(), None),
                };
                let mut opts = ImageTagOpts::builder().repo(repo);
                if let Some(tag) = tag {
                    opts = opts.tag(tag);
                }
                remote_image.tag(&opts.build()).await.d()?;
            }
        }

        Ok(())
    }
}
