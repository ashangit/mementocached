use crate::Responder;

pub mod connection;

pub type CommandProcess = (Vec<u8>, Responder<Vec<u8>>);
