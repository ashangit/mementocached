use crate::Responder;

pub type CommandProcess = (Vec<u8>, Responder<Vec<u8>>);
