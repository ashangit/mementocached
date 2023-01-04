use mementocached::protos::kv;

use argparse::{ArgumentParser, Store};
use protobuf::Message;
use rand::{distributions::Alphanumeric, Rng};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn client_action(server_socket: String) {
    let mut socket = TcpStream::connect(server_socket).await.unwrap();

    let key_suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(7)
        .map(char::from)
        .collect();

    let mut index = 0;
    loop {
        // Get
        let mut command = kv::Request::new();
        let mut get = kv::GetRequest::new();
        get.key = format!("{key_suffix}{index}");
        command.set_get(get);

        let slice = [
            command.compute_size().to_be_bytes().as_slice(),
            command.write_to_bytes().unwrap().as_slice(),
        ]
        .concat();
        socket.write_all(slice.as_slice()).await.unwrap();

        let mut buf = vec![0; 1024];
        socket.read(&mut buf).await.unwrap();

        // Set
        let mut command = kv::Request::new();
        let mut set = kv::SetRequest::new();
        set.key = format!("{key_suffix}{index}");
        set.value = Vec::from("nicovalue");
        command.set_set(set);

        let slice = [
            command.compute_size().to_be_bytes().as_slice(),
            command.write_to_bytes().unwrap().as_slice(),
        ]
        .concat();
        socket.write_all(slice.as_slice()).await.unwrap();

        let mut buf = vec![0; 1024];
        socket.read(&mut buf).await.unwrap();

        // Get
        let mut command = kv::Request::new();
        let mut get = kv::GetRequest::new();
        get.key = format!("{key_suffix}{index}");
        command.set_get(get);

        let slice = [
            command.compute_size().to_be_bytes().as_slice(),
            command.write_to_bytes().unwrap().as_slice(),
        ]
        .concat();
        socket.write_all(slice.as_slice()).await.unwrap();

        let mut buf = vec![0; 1024];
        socket.read(&mut buf).await.unwrap();

        // delete
        let mut command = kv::Request::new();
        let mut delete = kv::DeleteRequest::new();
        delete.key = format!("{key_suffix}{index}");
        command.set_delete(delete);

        let slice = [
            command.compute_size().to_be_bytes().as_slice(),
            command.write_to_bytes().unwrap().as_slice(),
        ]
        .concat();
        socket.write_all(slice.as_slice()).await.unwrap();

        let mut buf = vec![0; 1024];
        socket.read(&mut buf).await.unwrap();

        index += 1;
    }
}

fn main() -> io::Result<()> {
    let mut server_host = "127.0.0.1".to_string();
    let mut server_port = 6379;
    let mut worker_threads = num_cpus::get();
    let mut number_of_async_tasks = num_cpus::get() * 4;

    {
        // this block limits scope of borrows by ap.refer() method
        let mut argument_parser = ArgumentParser::new();
        argument_parser.refer(&mut server_host).add_option(
            &["--server-host"],
            Store,
            "Server host (default:127.0.0.1)",
        );
        argument_parser.refer(&mut server_port).add_option(
            &["--server-port"],
            Store,
            "Server port (default: 6379)",
        );
        argument_parser.refer(&mut worker_threads).add_option(
            &["--worker-threads"],
            Store,
            "Number of worker client threads to use (default: nb cores of the host)",
        );
        argument_parser
            .refer(&mut number_of_async_tasks)
            .add_option(
                &["--async-client"],
                Store,
                "Number of async tasks to use (default: nb cores of the host * 4)",
            );
        argument_parser.parse_args_or_exit();
    }

    let server_socket = format!("{server_host}:{server_port}");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_io()
        .thread_name_fn(|| {
            static ATOMIC_ID: AtomicUsize = AtomicUsize::new(0);
            let id = ATOMIC_ID.fetch_add(1, Ordering::SeqCst);
            format!("socket-writer-{}", id)
        })
        .build()?;

    rt.block_on(async {
        let mut joins = vec![];

        for _n in 0..number_of_async_tasks {
            let server_socket = server_socket.clone();
            let join = tokio::spawn(async move {
                client_action(server_socket).await;
            });
            joins.push(join);
        }

        for join in joins {
            join.await.unwrap();
        }
    });

    Ok(())
}
