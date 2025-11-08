#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("SerializationError: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Kube Error: {0}")]
    KubeError(#[from] kube::Error),

    #[error("Finalizer Error: {0}")]
    FinalizerError(#[from] Box<kube::runtime::finalizer::Error<Error>>),
}
