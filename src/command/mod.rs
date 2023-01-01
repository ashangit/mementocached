use crate::protos::kv;
use crate::Responder;

pub mod connection;

pub type CommandProcess = (Command, Responder<Vec<u8>>);

#[derive(Debug)]
pub enum Command {
    Get(Get),
    Set(Set),
    Delete(Delete),
}

#[derive(Debug)]
pub struct Get {
    pub request: kv::GetRequest,
}

#[derive(Debug)]
pub struct Set {
    pub request: kv::SetRequest,
}

#[derive(Debug)]
pub struct Delete {
    pub request: kv::DeleteRequest,
}
