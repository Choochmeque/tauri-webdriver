// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use crate::cli::Args;
use anyhow::Error;
use futures_util::TryFutureExt;
use http_body_util::{BodyExt, Either, Full};
use hyper::{
    body::{Bytes, Incoming},
    header::CONTENT_LENGTH,
    http::uri::Authority,
    service::service_fn,
    Method, Request, Response,
};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::{TokioExecutor, TokioIo},
    server::conn::auto,
};
#[cfg(target_os = "linux")]
use serde::Deserialize;
#[cfg(target_os = "linux")]
use serde_json::{json, Map, Value};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Child;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::sync::Arc;
use tokio::net::TcpListener;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use tokio::sync::RwLock;

const TAURI_OPTIONS: &str = "tauri:options";

type ResponseBody = Either<Full<Bytes>, Incoming>;

#[cfg(target_os = "linux")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TauriOptions {
    application: PathBuf,
    #[serde(default)]
    args: Vec<String>,
}

#[cfg(target_os = "linux")]
impl TauriOptions {
    fn into_native_object(self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert(
            "webkitgtk:browserOptions".into(),
            json!({"binary": self.application, "args": self.args}),
        );
        map
    }
}

#[cfg(target_os = "linux")]
async fn handle(
    client: Client<HttpConnector, Full<Bytes>>,
    req: Request<Incoming>,
    args: Args,
) -> Result<Response<ResponseBody>, Error> {
    // manipulate a new session to convert options to the native driver format
    let new_req: Request<Full<Bytes>> =
        if let (&Method::POST, "/session") = (req.method(), req.uri().path()) {
            let (mut parts, body) = req.into_parts();

            // get the body from the future stream and parse it as json
            let body = body.collect().await?.to_bytes().to_vec();
            let json: Value = serde_json::from_slice(&body)?;

            // manipulate the json to convert from tauri option to native driver options
            let json = map_capabilities(json);

            // serialize json and update the content-length header to be accurate
            let bytes = serde_json::to_vec(&json)?;
            parts.headers.insert(CONTENT_LENGTH, bytes.len().into());

            Request::from_parts(parts, Full::new(bytes.into()))
        } else {
            let (parts, body) = req.into_parts();
            let body = body.collect().await?.to_bytes().to_vec();
            Request::from_parts(parts, Full::new(body.into()))
        };

    client
        .request(forward_to_native_driver(new_req, args)?)
        .map_ok(|resp| resp.map(Either::Right))
        .err_into()
        .await
}

/// Transform the request to a request for the native webdriver server.
#[cfg(target_os = "linux")]
fn forward_to_native_driver(
    mut req: Request<Full<Bytes>>,
    args: Args,
) -> Result<Request<Full<Bytes>>, Error> {
    let host: Authority = {
        let headers = req.headers_mut();
        headers.remove("host").expect("hyper request has host")
    }
    .to_str()?
    .parse()?;

    let path = req
        .uri()
        .path_and_query()
        .expect("hyper request has uri")
        .clone();

    let uri = format!(
        "http://{}:{}{}",
        host.host(),
        args.native_port,
        path.as_str()
    );

    let (mut parts, body) = req.into_parts();
    parts.uri = uri.parse()?;
    Ok(Request::from_parts(parts, body))
}

/// only happy path for now, no errors
#[cfg(target_os = "linux")]
fn map_capabilities(mut json: Value) -> Value {
    let mut native = None;
    if let Some(capabilities) = json.get_mut("capabilities") {
        if let Some(always_match) = capabilities.get_mut("alwaysMatch") {
            if let Some(always_match) = always_match.as_object_mut() {
                if let Some(tauri_options) = always_match.remove(TAURI_OPTIONS) {
                    if let Ok(options) = serde_json::from_value::<TauriOptions>(tauri_options) {
                        native = Some(options.into_native_object());
                    }
                }

                if let Some(native) = native.clone() {
                    always_match.extend(native);
                }
            }
        }
    }

    if let Some(native) = native {
        if let Some(desired) = json.get_mut("desiredCapabilities") {
            if let Some(desired) = desired.as_object_mut() {
                desired.remove(TAURI_OPTIONS);
                desired.extend(native);
            }
        }
    }

    json
}

#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
pub async fn run(args: Args, mut _driver: Child) -> Result<(), Error> {
    #[cfg(unix)]
    let (signals_handle, signals_task) = {
        use futures_util::StreamExt;
        use signal_hook::consts::signal::*;

        let signals = signal_hook_tokio::Signals::new([SIGTERM, SIGINT, SIGQUIT])?;
        let signals_handle = signals.handle();
        let signals_task = tokio::spawn(async move {
            let mut signals = signals.fuse();
            #[allow(clippy::never_loop)]
            while let Some(signal) = signals.next().await {
                match signal {
                    SIGTERM | SIGINT | SIGQUIT => {
                        _driver
                            .kill()
                            .expect("unable to kill native webdriver server");
                        std::process::exit(0);
                    }
                    _ => unreachable!(),
                }
            }
        });
        (signals_handle, signals_task)
    };

    let address = std::net::SocketAddr::from(([127, 0, 0, 1], args.port));

    // the client we use to proxy requests to the native webdriver
    let client = Client::builder(TokioExecutor::new())
        .http1_preserve_header_case(true)
        .http1_title_case_headers(true)
        .retry_canceled_requests(false)
        .build_http();

    // set up a http1 server that uses the service we just created
    let srv = async move {
        if let Ok(listener) = TcpListener::bind(address).await {
            loop {
                let client = client.clone();
                let args = args.clone();
                if let Ok((stream, _)) = listener.accept().await {
                    let io = TokioIo::new(stream);

                    tokio::task::spawn(async move {
                        if let Err(err) = auto::Builder::new(TokioExecutor::new())
                            .http1()
                            .title_case_headers(true)
                            .preserve_header_case(true)
                            .serve_connection(
                                io,
                                service_fn(|request| handle(client.clone(), request, args.clone())),
                            )
                            .await
                        {
                            println!("Error serving connection: {err:?}");
                        }
                    });
                } else {
                    println!("accept new stream fail, ignore here");
                }
            }
        } else {
            println!("can not listen to address: {address:?}");
        }
    };
    srv.await;

    #[cfg(unix)]
    {
        signals_handle.close();
        signals_task.await?;
    }

    Ok(())
}

// ============================================================================
// Plugin Mode (macOS and Windows)
// ============================================================================

/// State for plugin mode - tracks the running Tauri app process
#[cfg(any(target_os = "macos", target_os = "windows"))]
struct PluginState {
    app_process: Option<Child>,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl PluginState {
    fn new() -> Self {
        Self { app_process: None }
    }
}

/// Handle requests in plugin mode
#[cfg(any(target_os = "macos", target_os = "windows"))]
async fn handle_plugin(
    client: Client<HttpConnector, Full<Bytes>>,
    req: Request<Incoming>,
    args: Args,
    state: Arc<RwLock<PluginState>>,
) -> Result<Response<ResponseBody>, Error> {
    // Handle session creation - launch the Tauri app
    if let (&Method::POST, "/session") = (req.method(), req.uri().path()) {
        let (mut parts, body) = req.into_parts();

        // get the body and parse tauri:options
        let body = body.collect().await?.to_bytes().to_vec();
        let json: Value = serde_json::from_slice(&body)?;

        // Extract tauri:options to get app path
        let app_path = extract_app_path(&json);

        if let Some(app_path) = app_path {
            // Launch the Tauri app
            let mut state = state.write().await;
            if state.app_process.is_some() {
                // Kill existing app if any
                if let Some(ref mut proc) = state.app_process {
                    let _ = proc.kill();
                }
            }

            let child = std::process::Command::new(&app_path)
                .env("TAURI_AUTOMATION", "true")
                .env("TAURI_WEBVIEW_AUTOMATION", "true")
                .spawn();

            match child {
                Ok(proc) => {
                    state.app_process = Some(proc);
                    drop(state);

                    // Wait for the plugin to be ready
                    let ready = wait_for_plugin(&args.native_host, args.native_port, 30).await;

                    if !ready {
                        return Ok(error_response(
                            "session not created",
                            "Plugin server not ready after timeout",
                        ));
                    }
                }
                Err(e) => {
                    return Ok(error_response(
                        "session not created",
                        &format!("Failed to launch Tauri app: {}", e),
                    ));
                }
            }
        }

        // Forward session creation to plugin (without tauri:options transformation)
        parts.headers.insert(CONTENT_LENGTH, body.len().into());
        let new_req = Request::from_parts(parts, Full::new(body.into()));

        return client
            .request(forward_to_plugin(new_req, &args)?)
            .map_ok(|resp| resp.map(Either::Right))
            .err_into()
            .await;
    }

    // Check if this is a session deletion request
    let is_session_delete = req.method() == &Method::DELETE && {
        let path = req.uri().path();
        let parts: Vec<&str> = path.split('/').collect();
        parts.len() == 3 && path.starts_with("/session/")
    };

    // Forward request to the plugin
    let (parts, body) = req.into_parts();
    let body = body.collect().await?.to_bytes().to_vec();
    let new_req = Request::from_parts(parts, Full::new(body.into()));

    let response = client
        .request(forward_to_plugin(new_req, &args)?)
        .map_ok(|resp| resp.map(Either::Right))
        .err_into()
        .await;

    // Kill the app AFTER the response is received for session deletion
    if is_session_delete {
        let mut state = state.write().await;
        if let Some(ref mut proc) = state.app_process {
            let _ = proc.kill();
        }
        state.app_process = None;
    }

    response
}

/// Extract app path from tauri:options in capabilities
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn extract_app_path(json: &Value) -> Option<PathBuf> {
    let capabilities = json.get("capabilities")?;
    let always_match = capabilities.get("alwaysMatch")?;
    let tauri_options = always_match.get(TAURI_OPTIONS)?;
    let application = tauri_options.get("application")?.as_str()?;
    Some(PathBuf::from(application))
}

/// Forward request to the plugin server
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn forward_to_plugin(
    mut req: Request<Full<Bytes>>,
    args: &Args,
) -> Result<Request<Full<Bytes>>, Error> {
    let host: Authority = {
        let headers = req.headers_mut();
        headers.remove("host").expect("hyper request has host")
    }
    .to_str()?
    .parse()?;

    let path = req
        .uri()
        .path_and_query()
        .expect("hyper request has uri")
        .clone();

    let uri = format!(
        "http://{}:{}{}",
        host.host(),
        args.native_port,
        path.as_str()
    );

    let (mut parts, body) = req.into_parts();
    parts.uri = uri.parse()?;
    Ok(Request::from_parts(parts, body))
}

/// Wait for the plugin server to be ready
#[cfg(any(target_os = "macos", target_os = "windows"))]
async fn wait_for_plugin(host: &str, port: u16, timeout_secs: u64) -> bool {
    let client: Client<HttpConnector, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let uri: hyper::Uri = format!("http://{}:{}/status", host, port)
        .parse()
        .expect("valid uri");

    while std::time::Instant::now() < deadline {
        let req = Request::builder()
            .method(Method::GET)
            .uri(uri.clone())
            .header("Host", format!("{}:{}", host, port))
            .body(Full::new(Bytes::new()));

        if let Ok(req) = req {
            if let Ok(resp) = client.request(req).await {
                if resp.status().is_success() {
                    return true;
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    false
}

/// Build a W3C WebDriver error response
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn error_response(error: &str, message: &str) -> Response<ResponseBody> {
    let body = json!({
      "value": {
        "error": error,
        "message": message
      }
    });
    let bytes = serde_json::to_vec(&body).unwrap();
    Response::builder()
        .status(500)
        .header("Content-Type", "application/json; charset=utf-8")
        .header(CONTENT_LENGTH, bytes.len())
        .body(Either::Left(Full::new(bytes.into())))
        .unwrap()
}

/// Run the server in plugin mode (macOS and Windows)
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[tokio::main(flavor = "current_thread")]
pub async fn run_plugin_mode(args: Args) -> Result<(), Error> {
    let state = Arc::new(RwLock::new(PluginState::new()));

    // Set up signal handling
    #[cfg(unix)]
    let (signals_handle, signals_task) = {
        use futures_util::StreamExt;
        use signal_hook::consts::signal::*;

        let signals = signal_hook_tokio::Signals::new([SIGTERM, SIGINT, SIGQUIT])?;
        let signals_handle = signals.handle();
        let state_for_signal = state.clone();

        let signals_task = tokio::spawn(async move {
            let mut signals = signals.fuse();
            while let Some(signal) = signals.next().await {
                match signal {
                    SIGTERM | SIGINT | SIGQUIT => {
                        // Kill the app process if running
                        let mut state = state_for_signal.write().await;
                        if let Some(ref mut proc) = state.app_process {
                            let _ = proc.kill();
                        }
                        std::process::exit(0);
                    }
                    _ => unreachable!(),
                }
            }
        });
        (signals_handle, signals_task)
    };

    #[cfg(windows)]
    let ctrl_c_task = {
        let state_for_signal = state.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                // Kill the app process if running
                let mut state = state_for_signal.write().await;
                if let Some(ref mut proc) = state.app_process {
                    let _ = proc.kill();
                }
                std::process::exit(0);
            }
        })
    };

    let address = std::net::SocketAddr::from(([127, 0, 0, 1], args.port));

    // the client we use to proxy requests to the plugin
    let client = Client::builder(TokioExecutor::new())
        .http1_preserve_header_case(true)
        .http1_title_case_headers(true)
        .retry_canceled_requests(false)
        .build_http();

    println!("tauri-driver running on port {}", args.port);
    println!("Plugin expected on port {}", args.native_port);

    let srv = async move {
        if let Ok(listener) = TcpListener::bind(address).await {
            loop {
                let client = client.clone();
                let args = args.clone();
                let state = state.clone();
                if let Ok((stream, _)) = listener.accept().await {
                    let io = TokioIo::new(stream);

                    tokio::task::spawn(async move {
                        if let Err(err) = auto::Builder::new(TokioExecutor::new())
                            .http1()
                            .title_case_headers(true)
                            .preserve_header_case(true)
                            .serve_connection(
                                io,
                                service_fn(|request| {
                                    handle_plugin(
                                        client.clone(),
                                        request,
                                        args.clone(),
                                        state.clone(),
                                    )
                                }),
                            )
                            .await
                        {
                            println!("Error serving connection: {err:?}");
                        }
                    });
                } else {
                    println!("accept new stream fail, ignore here");
                }
            }
        } else {
            println!("can not listen to address: {address:?}");
        }
    };
    srv.await;

    #[cfg(unix)]
    {
        signals_handle.close();
        signals_task.await?;
    }

    #[cfg(windows)]
    {
        ctrl_c_task.abort();
    }

    Ok(())
}
