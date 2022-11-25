use std::collections::BTreeMap;
use axum::{response::Html, routing::post, Router, body, middleware};
use axum::http::{HeaderMap, Request};
use std::net::SocketAddr;
use tower::ServiceBuilder;
use tower_http::ServiceBuilderExt;

mod imp;
mod digest;

#[tokio::main]
async fn main() {
    // build our application with a route
    let app = Router::new()
        .route("/", post(handler))
        .layer(
            ServiceBuilder::new()
                .map_request_body(body::boxed)
                .layer(middleware::from_fn(imp::verify_request_payload)),
        );
    // run it
    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    println!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}


async fn handler() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}
