//! Logging with `tracing`. Recent lines are also kept in memory for crash
//! reports (NFR-SEC-002).

use std::sync::Arc;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::persistence::crash::RecentLog;

/// Copies formatted log lines into the in-memory ring buffer.
struct RecentLayer(Arc<RecentLog>);

impl<S: tracing::Subscriber> Layer<S> for RecentLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        struct Visitor(String);
        impl tracing::field::Visit for Visitor {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0.push_str(&format!("{value:?}"));
                } else {
                    self.0.push_str(&format!(" {}={value:?}", field.name()));
                }
            }
        }
        let mut visitor = Visitor(String::new());
        event.record(&mut visitor);
        let meta = event.metadata();
        self.0
            .push(format!("{} {}: {}", meta.level(), meta.target(), visitor.0));
    }
}

/// Installs the global subscriber: warnings and above (or `RUST_LOG`) to
/// stderr, and everything from ArchiCraft at info level into the ring buffer.
pub fn init() -> Arc<RecentLog> {
    let recent = Arc::new(RecentLog::default());
    let stderr_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let recent_filter = EnvFilter::new("info,wgpu=warn,naga=warn,eframe=warn,egui=warn");
    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(stderr_filter),
        )
        .with(RecentLayer(recent.clone()).with_filter(recent_filter))
        .try_init();
    recent
}
