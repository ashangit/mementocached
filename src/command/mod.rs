use ahash::{HashMap, HashMapExt};
use protobuf::Message;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::protos::kv;
use crate::Responder;

pub type CommandProcess = (DBAction, Responder<Vec<u8>>);

#[derive(Debug)]
pub enum DBAction {
    Get(kv::GetRequest),
    Set(kv::SetRequest),
    Delete(kv::DeleteRequest),
}

#[derive(Clone)]
pub struct DB {
    data: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl Default for DB {
    fn default() -> Self {
        DB::new()
    }
}

impl DB {
    /// Create a DB
    /// Used to access a part of the whole database
    /// Rely on a HashMap
    ///
    /// # Return
    ///
    /// * DB
    ///
    pub fn new() -> Self {
        let data: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));

        DB { data }
    }

    /// Execute an action on the current DB
    ///
    /// # Arguments
    ///
    /// * `cmd` - DBAction: represent the action to execute with protobuf message containing
    /// information for the action to execute
    ///
    /// # Return
    ///
    /// * protobuf::Result<Vec<u8>> - bytes representing the protobuf message reply to the request
    ///
    pub async fn execute(&self, cmd: DBAction) -> protobuf::Result<Vec<u8>> {
        match cmd {
            DBAction::Get(get) => self.get(&get.key).await,
            DBAction::Set(set) => self.set(&set.key, &set.value).await,
            DBAction::Delete(delete) => self.delete(&delete.key).await,
        }
    }

    async fn get(&self, key: &String) -> protobuf::Result<Vec<u8>> {
        let data = self.data.lock().await;

        let mut reply = kv::GetReply::new();
        match data.get(key) {
            Some(x) => {
                reply.value = Vec::from(x.as_slice());
            }
            None => reply.err = "KO".to_string(),
        };
        reply.write_to_bytes()
    }

    async fn set(&self, key: &str, value: &[u8]) -> protobuf::Result<Vec<u8>> {
        let mut data = self.data.lock().await;

        data.insert(key.to_string(), value.to_vec());
        // Ignore errors
        let mut reply = kv::SetReply::new();
        reply.status = true;

        reply.write_to_bytes()
    }

    async fn delete(&self, key: &String) -> protobuf::Result<Vec<u8>> {
        let mut data = self.data.lock().await;

        data.remove(key);
        // Ignore errors
        let mut reply = kv::DeleteReply::new();
        reply.status = true;
        reply.write_to_bytes()
    }
}

#[cfg(test)]
mod tests {
    use crate::protos::kv::{DeleteReply, GetReply, SetReply};

    use super::*;

    async fn do_get(key: String, db: &DB) -> GetReply {
        let mut get_request = kv::GetRequest::new();
        get_request.key = key;
        let cmd = DBAction::Get(get_request);

        let reply = db.execute(cmd).await.unwrap();
        return Message::parse_from_bytes(reply.as_slice()).unwrap();
    }

    #[tokio::test]
    async fn get() {
        let db: DB = DB::new();
        db.data
            .lock()
            .await
            .insert("key".to_string(), "value".as_bytes().to_vec());

        let get_reply = do_get("key".to_string(), &db).await;
        assert_eq!(get_reply.value, "value".as_bytes().to_vec());

        let get_reply = do_get("key_not_found".to_string(), &db).await;
        assert_eq!(get_reply.value, "".as_bytes().to_vec());
        assert_eq!(get_reply.err, "KO");
    }

    #[tokio::test]
    async fn set() {
        let db: DB = DB::new();

        let mut set_request = kv::SetRequest::new();
        set_request.key = "key".to_string();
        set_request.value = "value".as_bytes().to_vec();
        let cmd = DBAction::Set(set_request);

        let reply = db.execute(cmd).await.unwrap();
        let set_reply: SetReply = Message::parse_from_bytes(reply.as_slice()).unwrap();
        assert!(set_reply.status);

        let tmp_db = db.data.lock().await;
        assert_eq!("value".as_bytes().to_vec(), *tmp_db.get("key").unwrap());
    }

    #[tokio::test]
    async fn delete() {
        let db: DB = DB::new();
        db.data
            .lock()
            .await
            .insert("key".to_string(), "value".as_bytes().to_vec());

        let mut delete_request = kv::DeleteRequest::new();
        delete_request.key = "key".to_string();
        let cmd = DBAction::Delete(delete_request);

        let reply = db.execute(cmd).await.unwrap();
        let set_reply: DeleteReply = Message::parse_from_bytes(reply.as_slice()).unwrap();
        assert!(set_reply.status);

        let tmp_db = db.data.lock().await;
        assert_eq!(None, tmp_db.get("key"));
    }
}
