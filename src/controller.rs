use std::{collections::BTreeMap, sync::Arc, time::Duration};

use k8s_openapi::api::apps::v1::Deployment;
use kube::{
    Api, Resource, ResourceExt,
    api::{ListParams, ObjectMeta, Patch, PatchParams},
    client::Client,
    runtime::{Controller, controller::Action, finalizer::Event as Finalizer, watcher},
};
use tokio_stream::StreamExt;

use crate::{
    crd::{Foo, FooStatus},
    error::Error,
};

const FIELD_MANAGER_NAME: &str = "sample-controller";
const FINALIZER_NAME: &str = "sample-controller/finalizer";

#[derive(Clone)]
pub struct Context {
    pub client: Client,
}

#[tracing::instrument(skip_all)]
async fn reconcile(foo: Arc<Foo>, ctx: Arc<Context>) -> Result<Action, Error> {
    let ns = foo.namespace().unwrap();
    let foos: Api<Foo> = Api::namespaced(ctx.client.clone(), &ns);

    info!(
        name = foo.name_any(),
        namespace = ns,
        "reconciling Foo resource"
    );

    kube::runtime::finalizer(&foos, FINALIZER_NAME, foo, |event| async {
        match event {
            Finalizer::Apply(foo) => foo.apply(ctx.clone()).await,
            Finalizer::Cleanup(foo) => foo.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}

impl Foo {
    #[instrument(skip_all)]
    async fn apply(&self, ctx: Arc<Context>) -> Result<Action, Error> {
        let client = ctx.client.clone();
        let ns = self.namespace().unwrap();

        let deployments: Api<Deployment> = Api::namespaced(client.clone(), &ns);
        let foos: Api<Foo> = Api::namespaced(client, &ns);

        let pp = PatchParams::apply(FIELD_MANAGER_NAME).force();
        let deployment_name = &self.spec.deployment_name;

        let dp = deployments
            .patch(deployment_name, &pp, &Patch::Apply(Deployment::from(self)))
            .await?;

        let status = Patch::Apply(Foo {
            status: Some(FooStatus {
                available_replicas: dp.spec.as_ref().unwrap().replicas.unwrap(),
            }),
            ..Default::default()
        });

        foos.patch_status(deployment_name, &pp, &status).await?;

        Ok(Action::await_change())
    }

    #[instrument(skip_all)]
    async fn cleanup(&self, _ctx: Arc<Context>) -> Result<Action, Error> {
        info!("cleaning up process before removing Foo resource");

        Ok(Action::await_change())
    }
}

#[instrument(skip_all)]
fn error_policy(_object: Arc<Foo>, error: &Error, _ctx: Arc<Context>) -> Action {
    error!(?error, "error occured on reconcile loop");
    Action::requeue(Duration::from_secs(10))
}

#[instrument(skip_all)]
pub async fn run(ctx: Context) -> Result<(), Error> {
    let foos = Api::<Foo>::all(ctx.client.clone());
    let deployments = Api::<Deployment>::all(ctx.client.clone());

    info!("checking if CRDs are installed");
    let _ = foos.list(&ListParams::default()).await?;
    info!("confirmed that CRDs are installed");

    let stream = Controller::new(foos, watcher::Config::default().any_semantic())
        .owns(deployments, watcher::Config::default())
        .shutdown_on_signal()
        .run(reconcile, error_policy, Arc::new(ctx));
    let mut stream = std::pin::pin!(stream);

    info!("starting up controller loop process");
    while let Some(res) = stream.next().await {
        if let Err(e) = res {
            error!(error = ?e, "error occured on controller loop");
        }
    }

    info!("controller has been terminated");

    Ok(())
}

impl From<&Foo> for Deployment {
    fn from(foo: &Foo) -> Self {
        use k8s_openapi::{
            api::{
                apps::v1::DeploymentSpec,
                core::v1::{Container, PodSpec, PodTemplateSpec},
            },
            apimachinery::pkg::apis::meta::v1::LabelSelector,
        };

        let oref = foo.controller_owner_ref(&()).unwrap();
        let labels = BTreeMap::from_iter([
            ("app".to_string(), "nginx".to_string()),
            ("controller".to_string(), foo.name_any().clone()),
        ]);

        Deployment {
            metadata: kube::api::ObjectMeta {
                name: Some(foo.spec.deployment_name.clone()),
                namespace: foo.namespace(),
                owner_references: Some(vec![oref]),
                ..Default::default()
            },
            spec: Some(DeploymentSpec {
                replicas: Some(foo.spec.replicas),
                selector: LabelSelector {
                    match_labels: Some(labels.clone()),
                    ..Default::default()
                },
                template: PodTemplateSpec {
                    metadata: Some(ObjectMeta {
                        labels: Some(labels),
                        ..Default::default()
                    }),
                    spec: Some(PodSpec {
                        containers: vec![Container {
                            name: "nginx".to_string(),
                            image: Some("nginx:latest".to_string()),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                },
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}
