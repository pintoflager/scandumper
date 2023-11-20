use axum::body::Bytes;
use s3::error::S3Error;
use s3::{Bucket, Region, creds::Credentials};
use tracing::{warn, info};
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

use common::Config;


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
                .unwrap_or_else(|_| "server=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let (dir, config) = match Config::paths_from_args() {
        Ok(t) => t,
        Err(e) => panic!("Failed to resolve root dir and config file: {}", e)
    };

    let config = match Config::from_path(&config, Some(&dir)) {
        Ok(c) => c,
        Err(e) => panic!("Failed to init config: {}", e),
    };

    let region = Region::Custom {
        region: config.s3.region.to_owned(),
        endpoint: format!("http://{}:{}", config.s3.host, config.s3.port),
    };

    let credentials = Credentials::new(
        config.s3.access_key.as_deref().or(
            std::env::var("S3_ACCESS_KEY").ok().as_deref()
        ),
        config.s3.secret_key.as_deref().or(
            std::env::var("S3_SECRET_KEY").ok().as_deref()
        ),
        None, None, None
    )
    .expect("Failed to init S3 credentials");

    let bucket = Bucket::new(&config.s3.bucket_name, region.clone(), credentials.clone())
        .expect("Failed to create S3 bucket")
        .with_path_style();

    // Poke minio to see if the bucket exists
    if let Err(e) = bucket.list("/".to_string(), None).await {
        match e {
            S3Error::Http(c, s) => match c {
                404 => {
                    warn!("Bucket doesn't exist yet?: {} / {}", c, s);

                    panic!("Bucket has to be created before running the server")
                }
                _ => panic!("Bucket of shit: {} / {}", c, s),
            },
            i => panic!("Bucket of shit: {}", i),
        }
    }

    let ip = match format!("{}:{}", config.server.host, config.server.port).parse::<SocketAddr>() {
        Ok(a) => a,
        Err(e) => panic!("Failed to join socket address from {:?}: {}", config.server, e),
    };

    let state = ServerState { bucket };

    // App with routes to list and read
    let app = Router::new()
        .route("/s3/get/*tail", get(s3_get_handler))
        .route("/s3/list/*tail", get(s3_object_handler))
        .route("/s3/index/*tail", get(s3_index_handler))
        .with_state(state);

    let server = axum::Server::bind(&ip)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>());

    info!("Listening on http://{}...", ip);

    if let Err(e) = server.await {
        panic!("Failed to run server: {}", e)
    }
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
        Ok(v) => {
            
            // info!("resp: {:?}", v);
            
            Responder::success(v.into_iter()
                .map(|i|i.common_prefixes.into_iter()
                    .flat_map(|v|v.into_iter()
                        .map(|p|p.prefix))
                    .collect::<Vec<String>>())
                .flatten()
                .collect()
            )
        },
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