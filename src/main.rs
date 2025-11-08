use kube::Client;
use tracing::info;

use sample_controller::{
    controller::{self, Context},
    error::Error,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().init();

    let client = Client::try_default()
        .await
        .expect("failed to create kube Client");

    let ctx = Context { client };

    info!("starting sample-controller");
    controller::run(ctx).await
}
