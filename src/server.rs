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
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Child;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

const TAURI_OPTIONS: &str = "tauri:options";

type ResponseBody = Either<Full<Bytes>, Incoming>;

/// State for plugin mode - tracks the running Tauri app process
struct PluginState {
    app_process: Option<Child>,
}

impl PluginState {
    fn new() -> Self {
        Self { app_process: None }
    }
}

/// Handle requests in plugin mode
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

        // Extract tauri:options to get app path and args
        let Some((app_path, app_args)) = extract_app_path_and_args(&json) else {
            return Ok(error_response(
                "session not created",
                "Missing tauri:options.application",
            ));
        };

        if !app_path.as_os_str().is_empty() {
            // Launch the Tauri app
            let mut state = state.write().await;
            if state.app_process.is_some() {
                // Kill existing app if any
                if let Some(ref mut proc) = state.app_process {
                    let _ = proc.kill();
                }
            }

            let mut cmd = std::process::Command::new(&app_path);
            cmd.env("TAURI_AUTOMATION", "true")
                .env("TAURI_WEBVIEW_AUTOMATION", "true")
                .args(&app_args);
            let child = cmd.spawn();

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
                        &format!("Failed to launch Tauri app: {e}"),
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
    let is_session_delete = req.method() == Method::DELETE && {
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
fn extract_app_path_and_args(json: &Value) -> Option<(PathBuf, Vec<String>)> {
    let capabilities = json.get("capabilities")?;
    let always_match = capabilities.get("alwaysMatch")?;
    let tauri_options = always_match.get(TAURI_OPTIONS)?;
    let application = tauri_options.get("application")?.as_str()?;
    let args = tauri_options
        .get("args")
        .and_then(|v| v.as_array())
        .map_or_else(Vec::new, |arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(std::string::ToString::to_string)
                .collect()
        });
    Some((PathBuf::from(application), args))
}

/// Forward request to the plugin server
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
async fn wait_for_plugin(host: &str, port: u16, timeout_secs: u64) -> bool {
    let client: Client<HttpConnector, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let uri: hyper::Uri = format!("http://{host}:{port}/status")
        .parse()
        .expect("valid uri");

    while std::time::Instant::now() < deadline {
        let req = Request::builder()
            .method(Method::GET)
            .uri(uri.clone())
            .header("Host", format!("{host}:{port}"))
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

/// Build a W3C `WebDriver` error response
fn error_response(error: &str, message: &str) -> Response<ResponseBody> {
    let body = json!({
      "value": {
        "error": error,
        "message": message
      }
    });
    let bytes = serde_json::to_vec(&body).unwrap_or_else(|_| b"internal error".to_vec());
    Response::builder()
        .status(500)
        .header("Content-Type", "application/json; charset=utf-8")
        .header(CONTENT_LENGTH, bytes.len())
        .body(Either::Left(Full::new(bytes.into())))
        .unwrap_or_else(|_| Response::new(Either::Left(Full::new(Bytes::from("internal error")))))
}

/// Run the server in plugin mode
#[tokio::main(flavor = "current_thread")]
pub async fn run_plugin_mode(args: Args) -> Result<(), Error> {
    let state = Arc::new(RwLock::new(PluginState::new()));

    // Set up signal handling
    #[cfg(unix)]
    let (signals_handle, signals_task) = {
        use futures_util::StreamExt;
        use signal_hook::consts::signal::{SIGINT, SIGQUIT, SIGTERM};

        let signals = signal_hook_tokio::Signals::new([SIGTERM, SIGINT, SIGQUIT])?;
        let signals_handle = signals.handle();
        let state_for_signal = state.clone();

        let signals_task = tokio::spawn(async move {
            let mut signals = signals.fuse();
            // Wait for any termination signal
            if signals.next().await.is_some() {
                // Kill the app process if running
                let mut state = state_for_signal.write().await;
                if let Some(ref mut proc) = state.app_process {
                    let _ = proc.kill();
                }
                std::process::exit(0);
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

    println!("tauri-webdriver running on port {}", args.port);
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
