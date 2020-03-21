//! Little proof-of-concept webservice in Rust, using experimental [tide] web framework.

// Make writing "unsafe" in code a compilation error. We should not need unsafe at all.
#![forbid(unsafe_code)]
// Warn on generally recommended lints that are not enabled by default.
#![warn(future_incompatible, rust_2018_idioms, unused, macro_use_extern_crate)]
// Warn when we write more code than necessary.
#![warn(unused_lifetimes, single_use_lifetimes, unreachable_pub, trivial_casts)]
// Warn when we don't implement (derive) commonly needed traits. May be too strict.
#![warn(missing_copy_implementations, missing_debug_implementations)]
// Turn on some extra Clippy (Rust code linter) warnings. Run `cargo clippy`.
#![warn(clippy::all)]

use crate::stateful::elasticsearch::WithElasticsearch;
use elasticsearch::Elasticsearch;
use env_logger::DEFAULT_FILTER_ENV;
use std::{env, io};

/// Module for endpoint handlers (also known as controllers).
mod handlers {
    pub(crate) mod city;
    pub(crate) mod fallback;
}
mod response;
/// Module for stateless services (that may depend on stateful ones from [stateful] module).
mod services {
    pub(crate) mod locations_repo;
}
/// Module for "stateful" services - those that need initialisation on startup and a living state.
mod stateful {
    pub(crate) mod elasticsearch;
}

/// Convenience type alias to be used by handlers.
type Request = tide::Request<AppState>;

#[async_std::main]
async fn main() -> io::Result<()> {
    // Set default log level to info and then init logging.
    if env::var(DEFAULT_FILTER_ENV).is_err() {
        env::set_var(DEFAULT_FILTER_ENV, "info");
    }
    pretty_env_logger::init_timed();

    let mut app = tide::with_state(AppState::new().await);
    app.middleware(tide::middleware::RequestLogger::new());

    app.at("/city/v1/get").get(handlers::city::get);

    app.at("/").all(handlers::fallback::not_found);
    app.at("/*").all(handlers::fallback::not_found);

    app.listen("0.0.0.0:8080").await
}

struct AppState {
    elasticsearch: Elasticsearch,
}

impl AppState {
    async fn new() -> Self {
        let elasticsearch = stateful::elasticsearch::new().await;

        Self { elasticsearch }
    }
}

impl WithElasticsearch for AppState {
    fn elasticsearch(&self) -> &Elasticsearch {
        &self.elasticsearch
    }
}
