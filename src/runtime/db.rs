use std::thread;

use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::info;

use crate::command::{CommandProcess, DB};
use crate::Error;

pub struct DBManagerRuntime {
    nb_workers: usize,
    worker_channel_buffer_size: usize,
    pub workers_channel: Vec<Sender<CommandProcess>>,
}

impl DBManagerRuntime {
    /// Create a new db manager runtime
    ///
    /// # Arguments
    ///
    /// * `nb_workers` - number of db worker to start
    ///
    /// # Return
    ///
    /// * Result<DBManagerRuntime, Error>
    ///
    pub fn new(nb_workers: usize) -> Result<Self, Error> {
        let workers_channel: Vec<Sender<CommandProcess>> = Vec::new();
        let worker_channel_buffer_size: usize = 32;

        Ok(DBManagerRuntime {
            nb_workers,
            worker_channel_buffer_size,
            workers_channel,
        })
    }

    /// Start the DB manager runtime
    /// This runtime is in charge of starting the DB worker runtime in a dedicated thread
    ///
    /// # Return
    ///
    /// * Result<(), Error>
    ///
    pub fn start(&mut self) -> Result<(), Error> {
        for worker_index in 0..self.nb_workers {
            let (tx, rx) = mpsc::channel::<CommandProcess>(self.worker_channel_buffer_size);
            self.workers_channel.push(tx);

            info!(
                "Start database worker {}/{}",
                worker_index + 1,
                self.nb_workers
            );
            let _ = thread::Builder::new()
                .name(format!("worker-{worker_index}"))
                .spawn(|| {
                    let mut worker_rt = DBWorkerRuntime::new().expect("Failed to init db worker");
                    worker_rt.run(rx).expect("Failure running db worker");
                });
        }
        Ok(())
    }
}

struct DBWorkerRuntime {
    rt: Runtime,
}

impl DBWorkerRuntime {
    /// Create a new db worker runtime
    ///
    /// # Return
    ///
    /// * Result<DBWorkerRuntime, Error>
    ///
    pub fn new() -> Result<Self, Error> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()?;

        Ok(DBWorkerRuntime { rt })
    }

    /// Run a db worker runtime
    /// It is in charge of managing one DB (HashMap containing a portion of the data)
    /// and to run the associated event loop that will execute action in front of that DB
    ///
    /// # Return
    ///
    /// * Result<(), Error>
    ///
    pub fn run(&mut self, mut rx: Receiver<CommandProcess>) -> Result<(), Error> {
        self.rt.block_on(async move {
            let db: DB = DB::new();

            while let Some((cmd, resp_tx)) = rx.recv().await {
                let db = db.clone();
                tokio::spawn(async move {
                    let reply: protobuf::Result<Vec<u8>> = db.execute(cmd).await;
                    resp_tx
                        .send(Ok(reply.expect("Not an expected protobuf Result message")))
                        .expect("Failed to send response to the socket worker");
                });
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::DBAction;
    use crate::protos::kv::{
        DeleteReply, DeleteRequest, GetReply, GetRequest, SetReply, SetRequest,
    };
    use protobuf::Message;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn call_db_worker() {
        let mut db_rt = DBManagerRuntime::new(2).unwrap();
        db_rt.start().unwrap();

        // Set request
        let (resp_tx, resp_rx) = oneshot::channel();
        let mut set = SetRequest::new();
        set.key = "key".to_string();
        set.value = Vec::from("value");
        let cmd = DBAction::Set(set);

        db_rt.workers_channel[0].send((cmd, resp_tx)).await.unwrap();

        if let Ok(result) = resp_rx.await {
            let reply: SetReply = Message::parse_from_bytes(result.unwrap().as_slice()).unwrap();
            assert!(reply.status);
        } else {
            panic!();
        }

        // Get request
        let (resp_tx, resp_rx) = oneshot::channel();
        let mut get = GetRequest::new();
        get.key = "key".to_string();
        let cmd = DBAction::Get(get);

        db_rt.workers_channel[0].send((cmd, resp_tx)).await.unwrap();

        if let Ok(result) = resp_rx.await {
            let reply: GetReply = Message::parse_from_bytes(result.unwrap().as_slice()).unwrap();
            assert_eq!(reply.value, "value".as_bytes().to_vec());
        } else {
            panic!();
        }

        // Delete request
        let (resp_tx, resp_rx) = oneshot::channel();
        let mut delete = DeleteRequest::new();
        delete.key = "key".to_string();
        let cmd = DBAction::Delete(delete);

        db_rt.workers_channel[0].send((cmd, resp_tx)).await.unwrap();

        if let Ok(result) = resp_rx.await {
            let reply: DeleteReply = Message::parse_from_bytes(result.unwrap().as_slice()).unwrap();
            assert!(reply.status);
        } else {
            panic!();
        }

        // Get request
        let (resp_tx, resp_rx) = oneshot::channel();
        let mut get = GetRequest::new();
        get.key = "key".to_string();
        let cmd = DBAction::Get(get);

        db_rt.workers_channel[0].send((cmd, resp_tx)).await.unwrap();

        if let Ok(result) = resp_rx.await {
            let reply: GetReply = Message::parse_from_bytes(result.unwrap().as_slice()).unwrap();
            assert_eq!(reply.err, "KO");
        } else {
            panic!();
        }
    }
}
