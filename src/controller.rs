use std::{collections::BTreeMap, sync::Arc, time::Duration};

use k8s_openapi::{
    api::{
        apps::v1::{Deployment, DeploymentSpec},
        core::v1::{Container, PodSpec, PodTemplateSpec},
    },
    apimachinery::pkg::apis::meta::v1::LabelSelector,
};
use kube::{
    Api, Resource, ResourceExt,
    api::{ListParams, ObjectMeta, Patch, PatchParams},
    client::Client,
    runtime::{Controller, controller::Action, finalizer::Event as Finalizer, watcher::Config},
};
use tokio_stream::StreamExt;

use crate::{crd::Foo, error::Error};

const FIELD_MANAGER_NAME: &str = "sample-controller";
const FINALIZER_NAME: &str = "sample-controller/finalizer";

#[derive(Clone)]
pub struct Context {
    pub client: Client,
}

async fn reconcile(foo: Arc<Foo>, ctx: Arc<Context>) -> Result<Action, Error> {
    let ns = foo.namespace().unwrap();
    let foos: Api<Foo> = Api::namespaced(ctx.client.clone(), &ns);

    println!("Reconciling Foo \"{}\" in {ns}", foo.name_any());
    kube::runtime::finalizer(&foos, FINALIZER_NAME, foo, |event| async {
        match event {
            Finalizer::Apply(foo) => foo.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(foo) => foo.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}

impl Foo {
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action, Error> {
        let client = ctx.client.clone();
        let ns = self.namespace().unwrap();

        let deployments: Api<Deployment> = Api::namespaced(client, &ns);

        deployments
            .patch(
                &self.spec.deployment_name,
                &PatchParams::apply(FIELD_MANAGER_NAME),
                &Patch::Apply(Deployment::from(self)),
            )
            .await?;

        Ok(Action::await_change())
    }

    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action, Error> {
        let deployments: Api<Deployment> =
            Api::namespaced(ctx.client.clone(), &self.namespace().unwrap());

        deployments
            .delete(&self.spec.deployment_name, &Default::default())
            .await?;

        Ok(Action::await_change())
    }
}

fn error_policy(_object: Arc<Foo>, error: &Error, _ctx: Arc<Context>) -> Action {
    eprintln!("error occured: {error:?}");
    Action::requeue(Duration::from_secs(10))
}

pub async fn run(ctx: Context) -> Result<(), Error> {
    let foos = Api::<Foo>::all(ctx.client.clone());
    let _ = foos.list(&ListParams::default()).await?;

    let stream = Controller::new(foos, Config::default().any_semantic())
        .shutdown_on_signal()
        .run(reconcile, error_policy, Arc::new(ctx));
    let mut stream = std::pin::pin!(stream);

    while let Some(res) = stream.next().await {
        if let Err(e) = res {
            eprintln!("error occured: {e:?}");
        }
    }

    Ok(())
}

impl From<&Foo> for Deployment {
    fn from(foo: &Foo) -> Self {
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
