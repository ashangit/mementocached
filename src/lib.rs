use tokio::sync::oneshot;

pub mod command;
pub mod metrics;
pub mod runtime;
pub mod protos {
    include!(concat!(env!("OUT_DIR"), "/protos/mod.rs"));
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Responder<T> = oneshot::Sender<Result<T, Error>>;
