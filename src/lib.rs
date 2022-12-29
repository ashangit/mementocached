use tokio::sync::oneshot;

pub mod command;
pub mod metrics;
pub mod reader;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Responder<T> = oneshot::Sender<std::result::Result<T, Error>>;
