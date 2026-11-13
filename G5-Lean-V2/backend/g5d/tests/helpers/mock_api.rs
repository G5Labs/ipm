// g5d test helpers — Lightweight axum-based mock API server.
// Records requests and returns canned responses for testing worker↔API communication.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::any,
    Router,
};
use serde_json::Value;

/// A recorded HTTP request captured by the mock server.
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    /// The raw query string, when the request had one.
    pub query: Option<String>,
    pub body: Option<Value>,
    pub headers: HashMap<String, String>,
    pub timestamp: Instant,
}

/// A canned response for a specific method + path combination.
#[derive(Debug, Clone)]
struct MockResponse {
    status: u16,
    body: Option<Value>,
}

type MockResponseKey = (String, String);
type MockResponseQueue = VecDeque<MockResponse>;
type MockResponseMap = HashMap<MockResponseKey, MockResponseQueue>;
type SharedMockResponses = Arc<Mutex<MockResponseMap>>;

/// Shared state for the mock API server.
#[derive(Clone)]
struct MockState {
    recorded_requests: Arc<Mutex<Vec<RecordedRequest>>>,
    canned_responses: SharedMockResponses,
}

/// Lightweight axum-based HTTP server that records requests and returns canned responses.
///
/// Used for testing worker↔API communication without a full g5-api + Postgres stack.
pub struct MockApiServer {
    pub addr: SocketAddr,
    pub recorded_requests: Arc<Mutex<Vec<RecordedRequest>>>,
    canned_responses: SharedMockResponses,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    server_thread: Option<JoinHandle<()>>,
}

impl MockApiServer {
    /// Starts the mock API server on a random port.
    ///
    /// The server records every incoming request and returns canned responses when
    /// configured via [`Self::when`]. Unconfigured endpoints return `200 OK` with `{}`.
    pub async fn start() -> Self {
        let recorded_requests = Arc::new(Mutex::new(Vec::new()));
        let canned_responses: SharedMockResponses = Arc::new(Mutex::new(HashMap::new()));

        let state = MockState {
            recorded_requests: Arc::clone(&recorded_requests),
            canned_responses: Arc::clone(&canned_responses),
        };

        let app = Router::new()
            .fallback(any(catch_all_handler))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind TCP listener for MockApiServer");
        let addr = listener.local_addr().expect("Failed to get local addr");

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("MockApiServer failed");
        });

        MockApiServer {
            addr,
            recorded_requests,
            canned_responses,
            shutdown_tx: Some(shutdown_tx),
            server_thread: None,
        }
    }

    /// Starts the mock API server on a dedicated background thread.
    ///
    /// This variant is safe to call from synchronous constructors and from
    /// tests running on either Tokio runtime flavor.
    pub fn start_blocking() -> Self {
        let recorded_requests = Arc::new(Mutex::new(Vec::new()));
        let canned_responses: SharedMockResponses = Arc::new(Mutex::new(HashMap::new()));

        let state = MockState {
            recorded_requests: Arc::clone(&recorded_requests),
            canned_responses: Arc::clone(&canned_responses),
        };

        let app = Router::new()
            .fallback(any(catch_all_handler))
            .with_state(state);

        let std_listener = std::net::TcpListener::bind("127.0.0.1:0")
            .expect("Failed to bind TCP listener for MockApiServer");
        std_listener
            .set_nonblocking(true)
            .expect("Failed to make MockApiServer listener non-blocking");
        let addr = std_listener.local_addr().expect("Failed to get local addr");

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let server_thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to build MockApiServer runtime");
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(std_listener)
                    .expect("Failed to convert MockApiServer listener");
                axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = shutdown_rx.await;
                    })
                    .await
                    .expect("MockApiServer failed");
            });
        });

        MockApiServer {
            addr,
            recorded_requests,
            canned_responses,
            shutdown_tx: Some(shutdown_tx),
            server_thread: Some(server_thread),
        }
    }

    /// Returns the base URL of the mock server (e.g., `"http://127.0.0.1:12345"`).
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Returns the base WebSocket URL of the mock server (e.g., `"ws://127.0.0.1:12345"`).
    pub fn ws_url(&self) -> String {
        format!("ws://{}", self.addr)
    }

    /// Returns a [`MockEndpoint`] builder for configuring a canned response for the
    /// given method + path combination.
    pub fn when(&self, method: &str, path: &str) -> MockEndpoint<'_> {
        MockEndpoint {
            server: self,
            method: method.to_uppercase(),
            path: path.to_string(),
        }
    }

    /// Asserts that the given method + path was called exactly `times` times.
    ///
    /// # Panics
    /// Panics with a descriptive message if the assertion fails.
    pub fn assert_called(&self, method: &str, path: &str, times: usize) {
        let requests = self.recorded_requests.lock().expect("Lock poisoned");
        let actual = requests
            .iter()
            .filter(|r| r.method.eq_ignore_ascii_case(method) && r.path == path)
            .count();
        assert_eq!(
            actual, times,
            "Expected {} {} to be called {} times, but was called {} times",
            method, path, times, actual
        );
    }

    /// Asserts that the given method + path was called with a body matching the
    /// provided matcher closure.
    ///
    /// # Panics
    /// Panics if no matching request is found or if the matcher returns `false`.
    pub fn assert_called_with_body(
        &self,
        method: &str,
        path: &str,
        body_matcher: impl Fn(&Value) -> bool,
    ) {
        let requests = self.recorded_requests.lock().expect("Lock poisoned");
        let matching = requests
            .iter()
            .find(|r| r.method.eq_ignore_ascii_case(method) && r.path == path);

        let request = matching.unwrap_or_else(|| {
            panic!(
                "No request found matching {} {}. Recorded requests: {:?}",
                method,
                path,
                requests
                    .iter()
                    .map(|r| format!("{} {}", r.method, r.path))
                    .collect::<Vec<_>>()
            )
        });

        let body = request
            .body
            .as_ref()
            .unwrap_or_else(|| panic!("Request {} {} has no body", method, path));

        assert!(
            body_matcher(body),
            "Body matcher failed for {} {}. Body was: {}",
            method,
            path,
            serde_json::to_string_pretty(body).unwrap_or_default()
        );
    }

    /// Returns all recorded requests matching the given method and path.
    pub fn get_requests(&self, method: &str, path: &str) -> Vec<RecordedRequest> {
        let requests = self.recorded_requests.lock().expect("Lock poisoned");
        requests
            .iter()
            .filter(|r| r.method.eq_ignore_ascii_case(method) && r.path == path)
            .cloned()
            .collect()
    }

    /// Clears all recorded requests.
    pub fn clear_requests(&self) {
        let mut requests = self.recorded_requests.lock().expect("Lock poisoned");
        requests.clear();
    }
}

impl Drop for MockApiServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.server_thread.take() {
            let _ = handle.join();
        }
    }
}

/// Builder for configuring a canned response for a specific method + path.
pub struct MockEndpoint<'a> {
    server: &'a MockApiServer,
    method: String,
    path: String,
}

impl<'a> MockEndpoint<'a> {
    /// Set the canned response status code and JSON body.
    pub fn respond_with(self, status: u16, body: Value) {
        let mut responses = self.server.canned_responses.lock().expect("Lock poisoned");
        responses.insert(
            (self.method, self.path),
            VecDeque::from([MockResponse {
                status,
                body: Some(body),
            }]),
        );
    }

    /// Set the canned response to return a specific status with no body.
    pub fn respond_status(self, status: u16) {
        let mut responses = self.server.canned_responses.lock().expect("Lock poisoned");
        responses.insert(
            (self.method, self.path),
            VecDeque::from([MockResponse { status, body: None }]),
        );
    }

    /// Configure a sequence of canned responses for the same method/path.
    ///
    /// Each request consumes the next response in the queue. Once only one
    /// response remains, the mock server reuses that final response for all
    /// subsequent calls.
    pub fn respond_sequence(self, responses_seq: Vec<(u16, Option<Value>)>) {
        let queue = responses_seq
            .into_iter()
            .map(|(status, body)| MockResponse { status, body })
            .collect::<VecDeque<_>>();
        let mut responses = self.server.canned_responses.lock().expect("Lock poisoned");
        responses.insert((self.method, self.path), queue);
    }
}

// ---------------------------------------------------------------------------
// Axum handler
// ---------------------------------------------------------------------------

/// Catch-all handler that records every request and returns canned or default responses.
async fn catch_all_handler(
    State(state): State<MockState>,
    request: Request<Body>,
) -> impl IntoResponse {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let query = request.uri().query().map(str::to_string);

    // Extract headers.
    let headers: HashMap<String, String> = request
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    // Extract body.
    let body_bytes = axum::body::to_bytes(request.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap_or_default();
    let body: Option<Value> = if body_bytes.is_empty() {
        None
    } else {
        serde_json::from_slice(&body_bytes).ok()
    };

    // Record the request.
    {
        let mut recorded = state.recorded_requests.lock().expect("Lock poisoned");
        recorded.push(RecordedRequest {
            method: method.clone(),
            path: path.clone(),
            query,
            body,
            headers,
            timestamp: Instant::now(),
        });
    }

    // Look up canned response.
    let response = {
        let mut responses = state.canned_responses.lock().expect("Lock poisoned");
        responses
            .get_mut(&(method.clone(), path.clone()))
            .and_then(|queue| {
                if queue.len() > 1 {
                    queue.pop_front()
                } else {
                    queue.front().cloned()
                }
            })
    };

    match response {
        Some(mock_resp) => {
            let status = StatusCode::from_u16(mock_resp.status).unwrap_or(StatusCode::OK);
            match mock_resp.body {
                Some(body) => (status, axum::Json(body)).into_response(),
                None => status.into_response(),
            }
        }
        None => {
            // Default: 200 OK with empty JSON object.
            (StatusCode::OK, axum::Json(serde_json::json!({}))).into_response()
        }
    }
}
