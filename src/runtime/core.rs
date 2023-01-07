use tokio::runtime::Runtime;
use tracing::error;

use crate::metrics::HttpEndpoint;
use crate::Error;

pub struct CoreRuntime {
    http_endpoint: HttpEndpoint,
    rt: Runtime,
}

impl CoreRuntime {
    /// Create a new core runtime
    ///
    /// # Arguments
    ///
    /// * `port` - the listen port
    ///
    /// # Return
    ///
    /// * Result<CoreRuntime, Error>
    ///
    pub fn new(port: u16) -> Result<Self, Error> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name("core")
            .build()?;

        let http_endpoint = HttpEndpoint::new(port).unwrap();

        Ok(CoreRuntime { http_endpoint, rt })
    }

    pub fn start(&mut self) -> Result<(), Error> {
        self.rt.block_on(async {
            if let Err(issue) = self.http_endpoint.start().await {
                error!("Issue to start prometheus http endpoint due to {}", issue);
                std::process::abort();
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[tokio::test]
    async fn start_http_endpoint() {
        let mut rt = CoreRuntime::new(0).unwrap();
        let port = rt.http_endpoint.listener.local_addr().unwrap().port();

        thread::Builder::new()
            .spawn(move || {
                rt.start().unwrap();
            })
            .unwrap();

        let resp = reqwest::get(format!("http://localhost:{port}/healthz"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(resp, "ok".to_string());

        let resp = reqwest::get(format!("http://localhost:{port}/metrics"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(resp.contains("# HELP process_cpu_seconds_total"));
    }
}
