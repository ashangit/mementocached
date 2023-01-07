extern crate core;

use tokio::sync::oneshot;

pub mod command;
pub mod connection;
pub mod metrics;

pub mod protos {
    include!(concat!(env!("OUT_DIR"), "/protos/mod.rs"));
}

pub mod runtime;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Responder<T> = oneshot::Sender<Result<T, Error>>;
