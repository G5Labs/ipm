//! Shared production/test listener boundary, including peer propagation.

use crate::{middleware, services::AppState};
use axum::Router;
use std::{future::Future, io, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;

pub async fn serve(
    listener: TcpListener,
    router: Router,
    state: Arc<AppState>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    // Preview translation must wrap the router, before route selection.
    let app = tower::Layer::layer(
        &axum::middleware::from_fn_with_state(state, middleware::preview_host_middleware),
        router,
    );
    let service = axum::ServiceExt::<axum::extract::Request>::into_make_service_with_connect_info::<
        SocketAddr,
    >(app);
    axum::serve(listener, service)
        .with_graceful_shutdown(shutdown)
        .await
}
