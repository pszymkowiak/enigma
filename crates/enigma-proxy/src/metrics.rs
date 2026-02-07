//! Prometheus metrics endpoint for enigma-proxy.

use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry, TextEncoder,
};
use std::net::SocketAddr;
use std::sync::LazyLock;

/// Global metrics registry.
pub struct Metrics {
    pub registry: Registry,
    pub requests_total: IntCounterVec,
    pub request_duration: HistogramVec,
    pub upload_bytes: IntCounterVec,
    pub download_bytes: IntCounterVec,
    pub errors_total: IntCounterVec,
    pub active_connections: IntGauge,
    pub storage_chunks: IntGauge,
}

pub static METRICS: LazyLock<Metrics> = LazyLock::new(|| {
    let registry = Registry::new();

    let requests_total = IntCounterVec::new(
        Opts::new("enigma_requests_total", "Total S3 API requests"),
        &["method", "status"],
    )
    .unwrap();

    let request_duration = HistogramVec::new(
        HistogramOpts::new(
            "enigma_request_duration_seconds",
            "Request duration in seconds",
        ),
        &["method"],
    )
    .unwrap();

    let upload_bytes = IntCounterVec::new(
        Opts::new("enigma_upload_bytes_total", "Total bytes uploaded"),
        &["provider"],
    )
    .unwrap();

    let download_bytes = IntCounterVec::new(
        Opts::new("enigma_download_bytes_total", "Total bytes downloaded"),
        &["provider"],
    )
    .unwrap();

    let errors_total =
        IntCounterVec::new(Opts::new("enigma_errors_total", "Total errors"), &["type"]).unwrap();

    let active_connections =
        IntGauge::new("enigma_active_connections", "Number of active connections").unwrap();

    let storage_chunks =
        IntGauge::new("enigma_storage_chunks", "Total number of stored chunks").unwrap();

    registry.register(Box::new(requests_total.clone())).unwrap();
    registry
        .register(Box::new(request_duration.clone()))
        .unwrap();
    registry.register(Box::new(upload_bytes.clone())).unwrap();
    registry.register(Box::new(download_bytes.clone())).unwrap();
    registry.register(Box::new(errors_total.clone())).unwrap();
    registry
        .register(Box::new(active_connections.clone()))
        .unwrap();
    registry.register(Box::new(storage_chunks.clone())).unwrap();

    Metrics {
        registry,
        requests_total,
        request_duration,
        upload_bytes,
        download_bytes,
        errors_total,
        active_connections,
        storage_chunks,
    }
});

fn render_metrics() -> Vec<u8> {
    let encoder = TextEncoder::new();
    let metric_families = METRICS.registry.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    buffer
}

/// Serve the /metrics endpoint on a separate port.
pub async fn serve_metrics(addr: SocketAddr) {
    use http_body_util::Full;
    use hyper::service::service_fn;
    use hyper::{Request, Response};

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Failed to bind metrics server on {addr}: {e}");
            return;
        }
    };

    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };

        tokio::spawn(async move {
            let service = service_fn(|_req: Request<hyper::body::Incoming>| async {
                let body = render_metrics();
                Ok::<_, hyper::Error>(
                    Response::builder()
                        .header("Content-Type", "text/plain; version=0.0.4")
                        .body(Full::new(bytes::Bytes::from(body)))
                        .unwrap(),
                )
            });

            let io = hyper_util::rt::TokioIo::new(stream);
            let builder =
                hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());
            let _ = builder.serve_connection(io, service).await;
        });
    }
}
