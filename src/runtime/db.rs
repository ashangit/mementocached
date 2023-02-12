use std::hash::Hash;
use std::thread;

use ahash::RandomState;
use protobuf::Message;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, error, info};

use crate::command::{CommandProcess, DBAction, DB};
use crate::connection::Connection;
use crate::metrics::NUMBER_OF_REQUESTS;
use crate::protos::kv;
use crate::protos::kv::Request;
use crate::runtime::socket::ClientStream;
use crate::{Error, Responder};

const HASHER: RandomState = RandomState::with_seeds(0, 0, 0, 0);

pub struct DBManagerRuntime {
    nb_workers: usize,
    worker_channel_buffer_size: usize,
    sockets_db_workers_rx: async_channel::Receiver<ClientStream>,
    pub sockets_db_workers_tx: async_channel::Sender<ClientStream>,
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
    pub fn new(nb_workers: usize) -> Self {
        let worker_channel_buffer_size: usize = 32;
        let (sockets_db_workers_tx, sockets_db_workers_rx) =
            async_channel::bounded(worker_channel_buffer_size);

        DBManagerRuntime {
            nb_workers,
            worker_channel_buffer_size,
            sockets_db_workers_rx,
            sockets_db_workers_tx,
        }
    }

    /// Start the DB manager runtime
    /// This runtime is in charge of starting the DB worker runtime in a dedicated thread
    ///
    /// # Return
    ///
    /// * Result<(), Error>
    ///
    pub fn start(&mut self) -> Result<(), Error> {
        let mut workers_db_action_tx: Vec<mpsc::Sender<CommandProcess>> = Vec::new();
        let (broadcast_channel_to_workers, _) =
            broadcast::channel::<Vec<mpsc::Sender<CommandProcess>>>(1);

        for worker_id in 0..self.nb_workers {
            let (db_action_tx, db_action_rx) =
                mpsc::channel::<CommandProcess>(self.worker_channel_buffer_size);
            workers_db_action_tx.push(db_action_tx);
            info!(
                "Start database worker {}/{}",
                worker_id + 1,
                self.nb_workers
            );

            let broadcast_channel_rcv_workers = broadcast_channel_to_workers.subscribe();
            let sockets_db_workers_rx = self.sockets_db_workers_rx.clone();
            let worker_thread_id = worker_id;
            let _ = thread::Builder::new()
                .name(format!("worker-{worker_id}"))
                .spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_io()
                        .build()
                        .unwrap();

                    rt.block_on(async move {
                        let mut worker_rt = DBWorkerRuntime::new(
                            worker_thread_id,
                            db_action_rx,
                            sockets_db_workers_rx,
                        );
                        worker_rt
                            .run(broadcast_channel_rcv_workers)
                            .await
                            .expect("Failure running db worker");
                    });
                });
        }

        broadcast_channel_to_workers
            .send(workers_db_action_tx.clone())
            .unwrap();
        Ok(())
    }
}

struct DBWorkerRuntime {
    worker_id: usize,
    db: DB,

    db_action_rx: mpsc::Receiver<CommandProcess>,
    sockets_db_workers_rx: async_channel::Receiver<ClientStream>,
}

impl DBWorkerRuntime {
    /// Create a new db worker runtime
    ///
    /// # Return
    ///
    /// * Result<DBWorkerRuntime, Error>
    ///
    pub fn new(
        worker_id: usize,
        db_action_rx: mpsc::Receiver<CommandProcess>,
        sockets_db_workers_rx: async_channel::Receiver<ClientStream>,
    ) -> Self {
        DBWorkerRuntime {
            worker_id,
            db: DB::new(),
            db_action_rx,
            sockets_db_workers_rx,
        }
    }

    /// Run a db worker runtime
    /// It is in charge of managing one DB (HashMap containing a portion of the data)
    /// and to run the associated event loop that will execute action in front of that DB
    ///
    /// # Return
    ///
    /// * Result<(), Error>
    ///
    pub async fn run(
        &mut self,
        mut broadcast_channel_rcv_workers: broadcast::Receiver<Vec<mpsc::Sender<CommandProcess>>>,
    ) -> Result<(), Error> {
        let workers_db_action_tx = broadcast_channel_rcv_workers.recv().await.unwrap();

        loop {
            tokio::select! {
                client_stream = self.sockets_db_workers_rx.recv() => {
                    let workers_db_action_tx = workers_db_action_tx.clone();
                    self.process_db_action_from_socket(client_stream.unwrap(), workers_db_action_tx);
                },
                Some((cmd, resp_tx)) = self.db_action_rx.recv() => {
                     self.process_db_action_from_channel(cmd, resp_tx);
                 }
            }
        }
    }

    fn process_db_action_from_channel(&mut self, cmd: DBAction, resp_tx: Responder<Vec<u8>>) {
        let db = self.db.clone();
        tokio::spawn(async move {
            let reply: protobuf::Result<Vec<u8>> = db.execute(cmd).await;
            resp_tx
                .send(Ok(reply.expect("Not an expected protobuf Result message")))
                .expect("Failed to send response to the socket worker");
        });
    }

    fn process_db_action_from_socket(
        &mut self,
        client_stream: ClientStream,
        workers_db_action_tx: Vec<mpsc::Sender<CommandProcess>>,
    ) {
        let db = self.db.clone();
        let local_worker_id = self.worker_id;

        tokio::spawn(async move {
            let mut connection = Connection::new(client_stream.stream).unwrap();

            loop {
                match connection.read_message().await {
                    Ok(Some(buffer)) => {
                        let request: Request = Message::parse_from_bytes(&buffer).unwrap();

                        let (worker_id, cmd) =
                            Self::process(request, workers_db_action_tx.len()).await;

                        let reply = if worker_id == local_worker_id {
                            debug!("Treat action locally");
                            db.execute(cmd).await.unwrap()
                        } else {
                            let (resp_tx, resp_rx) = oneshot::channel();
                            workers_db_action_tx[worker_id]
                                .send((cmd, resp_tx))
                                .await
                                .unwrap();

                            // Await the response
                            resp_rx.await.unwrap().unwrap()
                        };

                        // Write the response to the client
                        connection.write_message(reply).await.unwrap();
                    }
                    Ok(None) => {
                        info!(
                            client_addr = client_stream.addr,
                            "Connection closed by client"
                        );
                        break;
                    }
                    Err(issue) => {
                        error!(
                            client_addr = client_stream.addr,
                            issue = issue.to_string(),
                            "Failure reading message from client"
                        );
                        break;
                    }
                }
            }
        });
    }

    async fn process(request: Request, len_vec: usize) -> (usize, DBAction) {
        match request.command.unwrap() {
            kv::request::Command::Get(request) => {
                NUMBER_OF_REQUESTS.with_label_values(&["get"]).inc();

                // Send the GET request
                let worker_id: usize = Self::calculate_modulo(&request.key, len_vec);
                debug!(
                    key = request.key,
                    "Send get request to DB worker {}", worker_id
                );

                let cmd = DBAction::Get(request);
                (worker_id, cmd)
            }
            kv::request::Command::Set(request) => {
                NUMBER_OF_REQUESTS.with_label_values(&["set"]).inc();

                // Send the SET request
                let worker_id: usize = Self::calculate_modulo(&request.key, len_vec);
                debug!(
                    key = request.key,
                    "Send set request to DB worker {}", worker_id
                );

                let cmd = DBAction::Set(request);
                (worker_id, cmd)
            }
            kv::request::Command::Delete(request) => {
                NUMBER_OF_REQUESTS.with_label_values(&["delete"]).inc();

                // Send the DELETE request
                let worker_id: usize = Self::calculate_modulo(&request.key, len_vec);
                debug!(
                    key = request.key,
                    "Send delete request to DB worker {}", worker_id
                );

                let cmd = DBAction::Delete(request);
                (worker_id, cmd)
            }
        }
    }

    fn calculate_modulo<T: Hash>(t: T, len_vec: usize) -> usize {
        HASHER.hash_one(t) as usize % len_vec
    }
}

#[cfg(test)]
mod tests {
    use crate::protos::kv::{DeleteRequest, GetRequest, SetRequest};

    use super::*;

    #[test]
    fn compute_hash() {
        let hash = DBWorkerRuntime::calculate_modulo("test_key", 6);
        assert_eq!(hash, 0);
    }

    #[tokio::test]
    async fn process() {
        let len_vec = 6;
        let mut set = SetRequest::new();
        set.key = "key1".to_string();
        set.value = Vec::from("value");
        let mut request = Request::new();
        request.set_set(set);
        let (worker_id, db_action) = DBWorkerRuntime::process(request, len_vec).await;
        assert_eq!(worker_id, 2);
        let expected_action = match db_action {
            DBAction::Set(_) => true,
            _ => false,
        };
        assert!(expected_action);

        let mut get = GetRequest::new();
        get.key = "key".to_string();
        let mut request = Request::new();
        request.set_get(get);
        let (_, db_action) = DBWorkerRuntime::process(request, len_vec).await;
        let expected_action = match db_action {
            DBAction::Get(_) => true,
            _ => false,
        };
        assert!(expected_action);

        let mut delete = DeleteRequest::new();
        delete.key = "key".to_string();
        let mut request = Request::new();
        request.set_delete(delete);
        let (_, db_action) = DBWorkerRuntime::process(request, len_vec).await;
        let expected_action = match db_action {
            DBAction::Delete(_) => true,
            _ => false,
        };
        assert!(expected_action);
    }
}
