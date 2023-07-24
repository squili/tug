use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use futures_util::future::TryJoinAll;
use miette::Context;
use parking_lot::Mutex;
use podman_api::Podman;
use tokio::{sync::mpsc, task::JoinHandle};

use self::{
    container::ContainerAction,
    garbage::GarbageAction,
    image::{ImageAction, ResolvedImageRef},
    network::{NetworkAction, ResolvedNetworkRef},
    secret::{ResolvedSecretRef, SecretAction},
    volume::{ResolvedVolumeRef, VolumeAction},
};
use crate::{config::Config, logger::Logger, utils::IntoDiagnosticShorthand};

pub mod container;
pub mod garbage;
pub mod image;
pub mod network;
pub mod secret;
pub mod volume;

pub struct Executor {
    pub steps: Vec<Arc<Mutex<Step>>>,
    pub failures: Arc<Mutex<Vec<miette::Report>>>,
    completions_tx: mpsc::Sender<(usize, Option<miette::Report>)>,
    completions_rx: mpsc::Receiver<(usize, Option<miette::Report>)>,
    backtrack: Arc<Mutex<Vec<PostAction>>>,
    finalize: Arc<Mutex<Vec<PostAction>>>,
}

impl Executor {
    pub fn new() -> Self {
        let (completions_tx, completions_rx) = mpsc::channel(10);
        Executor {
            steps: Default::default(),
            failures: Default::default(),
            completions_tx,
            completions_rx,
            backtrack: Default::default(),
            finalize: Default::default(),
        }
    }

    pub fn new_step(&mut self, action: Action, depends_on: BTreeSet<usize>) -> usize {
        let id = self.steps.len();
        self.steps.push(Arc::new(Mutex::new(Step {
            id,
            action,
            depends_on,
            status: StepStatus::Queued,
        })));
        id
    }

    pub async fn execute(
        &mut self,
        config: &Config,
        logger: &Logger,
        service: Podman,
        root_directory: &Path,
    ) -> miette::Result<()> {
        const DEFAULT_CONCURRENCY_LIMIT: usize = 5;

        if self.steps.is_empty() {
            logger.trace("No steps");
            return Ok(());
        }

        let mut to_start = Vec::new();

        // queue initial steps
        logger.trace("Adding initial steps to to_start");
        for step in &self.steps {
            let step = step.lock();
            if step.depends_on.is_empty() {
                logger.trace(format!("Adding step {} to to_start", step.id));
                to_start.push(step.id);
            }
        }

        let mut concurrency_limit = DEFAULT_CONCURRENCY_LIMIT;
        let resolved_images: Arc<Mutex<BTreeMap<ResolvedImageRef, String>>> = Default::default();
        let resolved_networks: Arc<Mutex<BTreeMap<ResolvedNetworkRef, String>>> = Default::default();
        let resolved_volumes: Arc<Mutex<BTreeMap<ResolvedVolumeRef, String>>> = Default::default();
        let resolved_secrets: Arc<Mutex<BTreeMap<ResolvedSecretRef, String>>> = Default::default();

        logger.trace("Entering main loop");
        loop {
            // check if we're done
            if to_start.is_empty() && concurrency_limit == DEFAULT_CONCURRENCY_LIMIT {
                logger.trace("Done");
                break;
            }

            // start executing new steps
            while concurrency_limit > 1 {
                if let Some(id) = to_start.pop() {
                    logger.trace(format!("Executing {id}"));
                    concurrency_limit -= 1;
                    let step = &self.steps[id];
                    {
                        step.lock().status = StepStatus::Running;
                    }
                    let ctx = StepContext {
                        service: service.clone(),
                        resolved_images: resolved_images.clone(),
                        resolved_networks: resolved_networks.clone(),
                        resolved_volumes: resolved_volumes.clone(),
                        resolved_secrets: resolved_secrets.clone(),
                        group: config.group.clone(),
                        root_directory: root_directory.to_path_buf(),
                        backtrack: self.backtrack.clone(),
                        finalize: self.finalize.clone(),
                    };
                    tokio::spawn(Step::execute(ctx, step.clone(), self.completions_tx.clone()));
                } else {
                    logger.trace("Nothing left in to_start to execute");
                    break;
                }
            }

            // wait for the next step to complete
            logger.trace("Waiting for next step to complete");
            let (completed_id, failure_state) = self
                .completions_rx
                .recv()
                .await
                .expect("sender half should never be dropped before receiver half");
            logger.trace(format!("Step {completed_id} completed"));

            concurrency_limit += 1;

            if let Some(failure) = failure_state {
                let step = self.steps[completed_id].lock();
                logger.trace(format!("Step {completed_id} {step:?} reached failure state {failure:?}"));
                drop(step);
                self.failures.lock().push(failure);
                break;
            }

            // queue new steps
            for step in &self.steps {
                let mut step = step.lock();
                logger.trace(format!("Checking if step {} should be started", step.id));
                if to_start.contains(&step.id) {
                    logger.trace("Step already in to_start");
                    continue;
                }
                if step.status == StepStatus::Queued && step.depends_on.remove(&completed_id) && step.depends_on.is_empty() {
                    logger.trace(format!("Adding {} to to_start", step.id));
                    to_start.push(step.id);
                }
            }
        }

        while concurrency_limit < DEFAULT_CONCURRENCY_LIMIT {
            let (completed_id, failure_state) = self
                .completions_rx
                .recv()
                .await
                .expect("sender half should never be dropped before receiver half");
            logger.trace(format!("Post-completion from {completed_id}"));

            if let Some(failure) = failure_state {
                logger.trace(format!(
                    "Post-completion step {completed_id} reached failure state {failure:?}"
                ));
                self.failures.lock().push(failure);
            }

            concurrency_limit += 1;
        }

        if self.failures.lock().is_empty() {
            logger.log("Finalizing");
            logger.trace("Executing finalize");
            queue_post_action(&mut self.finalize, &service)
                .await
                .d()?
                .into_iter()
                .collect::<Result<_, _>>()
                .d()?;
        } else {
            logger.log("Failure state triggered, attempting to recover");
            logger.trace("Executing backtrack");
            let joins = queue_post_action(&mut self.backtrack, &service).await.d()?;

            if joins.iter().any(|join| join.is_err()) {
                logger.log("Error(s) while attempting to recover:");
                for join in joins {
                    if let Err(err) = join {
                        println!("{err:?}");
                    }
                }
            }

            logger.log("Error(s) while attempting to execute:");
            for failure in self.failures.lock().iter() {
                println!("{failure:?}");
            }
        }

        Ok(())
    }
}

pub struct StepContext {
    pub service: Podman,
    pub resolved_images: Arc<Mutex<BTreeMap<ResolvedImageRef, String>>>,
    pub resolved_networks: Arc<Mutex<BTreeMap<ResolvedNetworkRef, String>>>,
    pub resolved_volumes: Arc<Mutex<BTreeMap<ResolvedVolumeRef, String>>>,
    pub resolved_secrets: Arc<Mutex<BTreeMap<ResolvedSecretRef, String>>>,
    pub root_directory: PathBuf,
    pub group: String,
    pub backtrack: Arc<Mutex<Vec<PostAction>>>,
    pub finalize: Arc<Mutex<Vec<PostAction>>>,
}

#[derive(Debug)]
pub struct Step {
    pub id: usize,
    pub action: Action,
    pub status: StepStatus,
    depends_on: BTreeSet<usize>,
}

impl Step {
    pub async fn execute(ctx: StepContext, step: Arc<Mutex<Step>>, completions: mpsc::Sender<(usize, Option<miette::Report>)>) {
        let action = step.lock().action.clone();
        let failure_state = match action {
            Action::Container(action) => container::execute(&ctx, action).await.wrap_err("executing container step"),
            Action::Image(action) => image::execute(&ctx, action).await.wrap_err("executing image step"),
            Action::Garbage(action) => garbage::execute(&ctx, action).await.wrap_err("executing garbage step"),
            Action::Network(action) => network::execute(&ctx, action).await.wrap_err("executing network step"),
            Action::Volume(action) => volume::execute(&ctx, action).await.wrap_err("executing volume step"),
            Action::Secret(action) => secret::execute(&ctx, action).await.wrap_err("executing secret step"),
        }
        .err();

        let id = {
            let mut step = step.lock();
            step.status = StepStatus::Complete;
            step.id
        };
        completions.send((id, failure_state)).await.unwrap();
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum StepStatus {
    Queued,
    Running,
    Complete,
}

#[derive(Clone, Debug)]
pub enum Action {
    Container(ContainerAction),
    Image(ImageAction),
    Garbage(GarbageAction),
    Network(NetworkAction),
    Volume(VolumeAction),
    Secret(SecretAction),
}

pub enum PostAction {
    DeleteContainer { id: String },
    RestartContainer { id: String },
    DeleteNetwork { id: String },
    DeleteVolume { name: String },
}

fn queue_post_action(
    actions: &mut Arc<Mutex<Vec<PostAction>>>,
    service: &Podman,
) -> TryJoinAll<JoinHandle<Result<(), podman_api::Error>>> {
    futures_util::future::try_join_all(
        std::mem::take(Arc::get_mut(actions).unwrap())
            .into_inner()
            .into_iter()
            .map(|action| {
                let service = service.clone();
                match action {
                    PostAction::DeleteContainer { id } => {
                        tokio::spawn(async move { service.containers().get(id).remove().await })
                    }
                    PostAction::RestartContainer { id } => {
                        tokio::spawn(async move { service.containers().get(id).start(None).await })
                    }
                    PostAction::DeleteNetwork { id } => {
                        tokio::spawn(async move { service.networks().get(id).remove().await.map(|_| {}) })
                    }
                    PostAction::DeleteVolume { name } => tokio::spawn(async move { service.volumes().get(name).remove().await }),
                }
            }),
    )
}
