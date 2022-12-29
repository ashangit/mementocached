use argparse::{ArgumentParser, Store, StoreTrue};
use asyncached::Error;

use std::thread;

use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use tracing::error;

use asyncached::command::CommandProcess;

use asyncached::metrics::init_prometheus_http_endpoint;
use asyncached::reader::SocketReader;

fn main() -> Result<(), Error> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let mut cpus = num_cpus::get();
    let mut port = 6379;
    let mut http_port = 8080;
    let mut tokio_console = false;

    {
        // this block limits scope of borrows by ap.refer() method
        let mut argument_parser = ArgumentParser::new();
        argument_parser.refer(&mut cpus).add_option(
            &["--cores"],
            Store,
            "Number of cores to use (default: nb cores of the host",
        );
        argument_parser
            .refer(&mut port)
            .add_option(&["--port"], Store, "DB port (default: 6379)");
        argument_parser.refer(&mut http_port).add_option(
            &["--http-port"],
            Store,
            "Http port for metrics endpoint (default: 8080)",
        );
        argument_parser.refer(&mut tokio_console).add_option(
            &["--tokio-console"],
            StoreTrue,
            "Enable console subscriber for the tokio console (default: false)",
        );
        argument_parser.parse_args_or_exit();
    }

    // Init tokio console subscriber if enabled
    // Used to debug trace async task with https://github.com/tokio-rs/console
    if tokio_console {
        console_subscriber::init();
    }

    // TODO
    // each thread manage it's own hashmap

    // Init mono thread tokio scheduler
    let current_thread_runtime_res = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .thread_name("core")
        .build();

    match current_thread_runtime_res {
        Ok(current_thread_runtime) => {
            // Init prometheus http endpoint
            current_thread_runtime.spawn(async move {
                if let Err(issue) = init_prometheus_http_endpoint(http_port).await {
                    error!("Issue to start prometheus http endpoint due to {}", issue);
                    std::process::abort();
                }
            });
        }
        Err(issue) => {
            error!(
                "Issue starting multi-threaded tokio scheduler due to: {}",
                issue
            );
            //return Err(1);
        }
    };

    let mut workers_channel = Vec::new();
    for worker_index in 1..=cpus {
        let (tx, rx) = mpsc::channel::<CommandProcess>(32);

        //let tx_local = tx.clone();
        workers_channel.push(tx);

        // Spawn Worker threads
        let _ = thread::Builder::new()
            .name(format!("Worker {worker_index}"))
            .spawn(|| {
                let _ = worker(rx);
            });
    }

    let mut socket_reader = SocketReader::new(format!("127.0.0.1:{port}"), workers_channel)?;
    socket_reader.start()
}

fn worker(_rx: Receiver<CommandProcess>) -> Result<(), Error> {
    let _worker_rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()?;
    Ok(())
}
