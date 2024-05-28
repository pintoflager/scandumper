use axum::body::Bytes;
use s3::Bucket;
use tokio::net::TcpListener;
use tracing::{debug, info};
use std::net::SocketAddr;
use std::str::FromStr;
use serde::Serialize;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use axum::{
    extract::{State, Path, Json},
    http::{StatusCode, header::{HeaderName, HeaderValue, HeaderMap}},
    response::IntoResponse,
    routing::get,
    Router
};

use config::{Config, ObjectStore};


#[derive(Clone)]
struct ServerState {
    bucket: Bucket,
}

#[derive(Serialize)]
struct Responder<T: serde::Serialize> {
    status: String,
    data: T,
}

impl<T: serde::Serialize> Responder<T> {
    fn error(value: T, message: anyhow::Error) -> Json<Responder<T>> {
        let response = Self {
            status: format!("{} ({})", StatusCode::INTERNAL_SERVER_ERROR.to_string(), message),
            data: value
        };

        Json(response)
    }
    fn success(value: T) -> Json<Responder<T>> {
        let response = Self {
            status: StatusCode::OK.to_string(),
            data: value
        };

        Json(response)
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "imgserver=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut config = match Config::new() {
        Ok(c) => c,
        Err(e) => panic!("Failed to init config: {}", e),
    };

    debug!("Config loaded...");

    let server_config = match config.server() {
        Ok(c) => c,
        Err(e) => panic!("Incomplete config for server: {}", e),
    };

    debug!("Server config loaded...");

    let export_config = match config.export() {
        Ok(e) => e,
        Err(e) => panic!("Incomplete config for export: {}", e),
    };

    debug!("Export config loaded...");

    let s3_store = match export_config.s3 {
        true => {
            let s3_config = match config.s3_mut() {
                Ok(c) => c,
                Err(e) => panic!("Incomplete config for S3: {}", e),
            };

            // Verify that our bucket exists
            if let Err(e) = ObjectStore::init_from(s3_config, false).await {
                panic!("Object store loading from config failed: {}", e)
            }

            debug!("S3 config loaded...");

            ObjectStore::get(&s3_config).unwrap()
        },
        false => panic!("Server only reads objects from S3"),
    };

    let ip = match format!("{}:{}", server_config.host, server_config.port).parse::<SocketAddr>() {
        Ok(a) => a,
        Err(e) => panic!("Failed to join socket address from {:?}: {}", config.server, e),
    };

    let state = ServerState { bucket: s3_store.0 };

    // App with routes to list and read
    let app = Router::new()
        .route("/s3/get/*tail", get(s3_get_handler))
        .route("/s3/list/*tail", get(s3_object_handler))
        .route("/s3/index/*tail", get(s3_index_handler))
        .with_state(state);

    let listener = match TcpListener::bind(&ip).await {
        Ok(l) => l,
        Err(e) => panic!("Unable to start listener: {}", e),
    };
    
    info!("Listening on http://{}...", ip);
    
    axum::serve(listener, app).await.unwrap()
}

async fn s3_object_handler(Path(tail): Path<String>, state: State<ServerState>)
-> Json<Responder<Vec<String>>> {
    let delimiter = Some(String::from("/"));
    let prefix = format!("{}/", tail);

    match state.bucket.list(prefix, delimiter).await {
        Ok(v) => {
            
            // info!("resp: {:?}", v);
            
            Responder::success(v.into_iter()
                .map(|i|i.contents.into_iter()
                    .map(|o|o.key)
                    .collect::<Vec<String>>())
                .flatten()
                .collect()
            )
        },
        Err(e) => return Responder::error(vec![], e.into()),
    }
}

async fn s3_index_handler(Path(tail): Path<String>, state: State<ServerState>)
-> Json<Responder<Vec<String>>> {
    let delimiter = Some(String::from("/"));
    let prefix = format!("{}/", tail);

    match state.bucket.list(prefix, delimiter).await {
        Ok(v) => Responder::success(v.into_iter()
            .map(|i|i.common_prefixes.into_iter()
                .flat_map(|v|v.into_iter()
                    .map(|p|p.prefix))
                .collect::<Vec<String>>())
            .flatten()
            .collect()
        ),
        Err(e) => return Responder::error(vec![], e.into()),
    }
}

async fn s3_get_handler(Path(tail): Path<String>, state: State<ServerState>)
-> impl IntoResponse {
    let prefix = format!("{}", tail);
    let mut headers = HeaderMap::new();

    match state.bucket.get_object(prefix).await {
        Ok(v) => {
            for (k, v) in v.headers() {
                if ["content-type", "content-length"].contains(&k.to_lowercase().as_str()) {
                    headers.insert(
                        HeaderName::from_str(&k).unwrap(),
                        HeaderValue::from_str(&v).unwrap()
                    );
                }
            }
            
            (headers, v.to_vec()).into_response()
            
        },
        Err(_e) => (headers, Bytes::default()).into_response(),
    }
}
