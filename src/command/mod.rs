use crate::protos::kv;
use crate::Responder;
use ahash::HashMap;

use protobuf::Message;
use tokio::sync::MutexGuard;

pub mod connection;

pub type CommandProcess = (DBAction, Responder<Vec<u8>>);

#[derive(Debug)]
pub enum DBAction {
    Get(Get),
    Set(Set),
    Delete(Delete),
}

#[derive(Debug)]
pub struct Get {
    pub request: kv::GetRequest,
}

impl Get {
    pub fn execute(&self, db: MutexGuard<HashMap<String, Vec<u8>>>) -> protobuf::Result<Vec<u8>> {
        let mut reply = kv::GetReply::new();
        match db.get(&self.request.key) {
            Some(x) => {
                reply.value = Vec::from(x.as_slice().clone());
            }
            None => reply.err = "KO".to_string(),
        };
        reply.write_to_bytes()
    }
}

#[derive(Debug)]
pub struct Set {
    pub request: kv::SetRequest,
}

impl Set {
    pub fn execute(
        &self,
        mut db: MutexGuard<HashMap<String, Vec<u8>>>,
    ) -> protobuf::Result<Vec<u8>> {
        db.insert(self.request.key.clone(), self.request.value.to_vec());
        // Ignore errors
        let reply = kv::SetReply::new();
        //        reply. (true);

        reply.write_to_bytes()
    }
}

#[derive(Debug)]
pub struct Delete {
    pub request: kv::DeleteRequest,
}

impl Delete {
    pub fn execute(
        &self,
        mut db: MutexGuard<HashMap<String, Vec<u8>>>,
    ) -> protobuf::Result<Vec<u8>> {
        db.remove(self.request.key.as_str());
        // Ignore errors
        let reply = kv::DeleteReply::new();
        //reply.set_status(true);

        reply.write_to_bytes()
    }
}
