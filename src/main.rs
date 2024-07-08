use std::{collections::HashMap, process::exit, sync::Arc, time::Duration};

use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use tokio::{process::Command, sync::Mutex};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use autopilot::config::{parse_config, Config, Project};

type Projects = HashMap<String, Project>;
type Locks = HashMap<String, Mutex<bool>>;

struct AppState {
    projects: Projects,
    locks: Locks,
    #[allow(dead_code)] // TODO
    config: Config,
}

#[derive(Deserialize)]
struct RegistryPackage {
    name: String,
    // namespace: String,
}

#[derive(Deserialize)]
struct GitHubPayload {
    // action: String,
    registry_package: Option<RegistryPackage>,
}

// #[derive(Deserialize)]
// struct GitHubEvent {
//     event: String,
//     payload: GitHubPayload,
// }

#[axum::debug_handler]
async fn webhook_handler(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
    Json(payload): Json<GitHubPayload>,
) {
    let project = state.projects.get(&token);

    if project.is_none() {
        tracing::debug!("Received invalid token");
        return;
    }

    let project = project.unwrap();
    let compose_path = project.compose_path();
    let compose_path = compose_path.to_str().unwrap();
    tracing::debug!("Received event for compose file at {}", compose_path);

    let registry_package = match payload.registry_package {
        Some(p) => p,
        None => {
            tracing::debug!("Received malformed body from GitHub (?)");
            return;
        }
    };

    match &project.package_names {
        Some(n) => {
            if n.contains(&registry_package.name) {
                tracing::debug!("Received unknown package name ({})", registry_package.name,);
                return;
            }
        }
        None => (),
    }

    // for some reason github seems to push multiple package
    // publish events for a single build workflow run (all at once too),
    // so I have a lock here that should only pull and restart the compose
    // application for one of them.

    let lock_mutex = state.locks.get(&token).unwrap();
    let mut lock = lock_mutex.lock().await;

    if *lock {
        tracing::debug!("Already running, skipping",);
        return;
    }

    *lock = true;

    drop(lock);

    tracing::debug!("Waiting 5 seconds before pulling");
    tokio::time::sleep(Duration::from_secs(5)).await;

    tracing::debug!("Pulling");

    let output = Command::new("docker")
        .args(&["compose", "-f", compose_path, "pull"])
        .output()
        .await
        .unwrap();

    tracing::debug!("{}", output.status);
    tracing::debug!("{}", String::from_utf8(output.stdout).unwrap());
    tracing::debug!("{}", String::from_utf8(output.stderr).unwrap());

    tracing::debug!("Restarting");

    let output = Command::new("docker")
        .args(&["compose", "-f", compose_path, "restart"])
        .output()
        .await
        .unwrap();

    tracing::debug!("{}", output.status);
    tracing::debug!("{}", String::from_utf8(output.stdout).unwrap());
    tracing::debug!("{}", String::from_utf8(output.stderr).unwrap());

    tracing::debug!("All done");

    let mut lock = lock_mutex.lock().await;
    *lock = false;
}

async fn index_handler() -> String {
    format!("autopilot v{}", env!("CARGO_PKG_VERSION"))
}

#[tokio::main]
async fn main() {
    // init logging
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", format!("debug,hyper=info,mio=info"));
    }

    tracing_subscriber::fmt::init();

    let config = match parse_config() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("{}", e);
            exit(1)
        }
    };

    // build map of projects
    let mut projects: Projects = HashMap::new();
    let mut locks: Locks = HashMap::new();

    for project in config.clone().projects {
        if projects.contains_key(&project.token) {
            tracing::error!("Duplicate tokens found in config. Project tokens must be unique.");
            exit(1);
        }

        projects.insert(project.token.clone(), project.clone());
        locks.insert(project.token.clone(), Mutex::new(false));
    }

    let app_state = Arc::new(AppState {
        projects,
        locks,
        config: config.clone(),
    });

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/webhooks/:token/github", post(webhook_handler))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
        .with_state(app_state);

    // run our app with hyper
    let listener = tokio::net::TcpListener::bind(format!("{}:{}", config.host, config.port))
        .await
        .unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
