use kube::Client;

use sample_controller::{
    controller::{self, Context},
    error::Error,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let client = Client::try_default()
        .await
        .expect("failed to create kube Client");

    let ctx = Context { client };

    println!("starting sample-controller");
    controller::run(ctx).await
}
