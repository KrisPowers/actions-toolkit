use tracing_subscriber::{fmt, EnvFilter};

pub fn init() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,actions_toolkit_backend=debug,tower_http=info"));

    fmt().with_env_filter(filter).with_target(true).init();
}
