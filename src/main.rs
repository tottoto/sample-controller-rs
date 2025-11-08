use kube::{CustomResource, CustomResourceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn main() {
    let crd = serde_yaml::to_string(&Foo::crd()).unwrap();
    println!("{crd}");
}

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    kind = "Foo",
    group = "samplecontroller.k8s.io",
    version = "v1alpha1",
    namespaced,
    annotation("api-approved.kubernetes.io", "unapproved, experimental-only"),
    status = "FooStatus"
)]
pub struct FooSpec {
    pub deployment_name: String,
    pub replicas: i32,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct FooStatus {
    pub available_replicas: i32,
}
