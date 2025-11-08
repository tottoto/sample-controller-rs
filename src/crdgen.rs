use kube::CustomResourceExt;

use sample_controller::crd::Foo;

fn main() {
    let crd = serde_yaml::to_string(&Foo::crd()).unwrap();
    println!("{crd}");
}
