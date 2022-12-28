use argparse::{ArgumentParser, Store, StoreTrue};
use tracing::error;

use asyncached::metrics::init_prometheus_http_endpoint;

fn main() -> Result<(), i32> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let mut http_port = 8080;
    let mut tokio_console = false;

    {
        // this block limits scope of borrows by ap.refer() method
        let mut argument_parser = ArgumentParser::new();
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
    // init docker reader with channel to thread
    // each thread manage it's own hashmap

    // Init mono thread tokio scheduler
    let current_thread_runtime_res = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .thread_name("Core")
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
            return Err(1);
        }
    };

    Ok(())
}
