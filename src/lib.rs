use tokio::sync::oneshot;

pub mod command;
pub mod metrics;
pub mod runtime;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Responder<T> = oneshot::Sender<Result<T, Error>>;
