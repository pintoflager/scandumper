mod resizer;
mod jpeg;
mod png;
mod object_store;

use std::path::PathBuf;

use tokio::task::JoinSet;
use tracing::{error, debug, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use walkdir::WalkDir;

use common::Config;
use resizer::*;
use object_store::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "updater=info".into()),
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

    // Init object store from config
    let obj_store = match ObjectStore::init(&config) {
        Ok(i) => i,
        Err(e) => panic!("Object store setup failed: {}", e),
    };

    // Verify that our bucket exists
    match Objects::from(&obj_store) {
        Ok(mut i) => match i.verify_existence(true).await {
            Ok(_) => debug!("S3 bucket found / created."),
            Err(e) => panic!("Failed to validate S3 bucket: {}", e),
        },
        Err(e) => panic!("Failed to open S3 bucket: {}", e),
    };

    let mut queue = vec![];
    let path_prefix = config.prefix.unwrap_or(PathBuf::new());

    debug!("Iterating image ({}) dir(s), preparing resizer queue...", config.dirs.len());

    for p in config.dirs.iter() {
        let subdir = match p.is_dir() {
            true => p.to_owned(),
            false => {
                let mut path = dir.to_owned();
                path.push(p);

                match path.is_dir() {
                    true => path,
                    false => panic!("Unable to figure out dir path: {:?}", path)
                }
            }
        };

        for i in WalkDir::new(&subdir) {
            let e = match i {
                Ok(x) => x,
                Err(e) => {
                    error!("Failed to read dir entry: {}", e);
                    continue;
                }
            };

            let file = match e.path().is_file() {
                true => e.into_path(),
                false => {
                    debug!("Skipping dir entry as it's not a file: {:?}", e);
                    continue;
                },
            };

            info!("Proceeding to image resizer with {:?}", &file);
            let mut path = path_prefix.to_owned();

            match file.strip_prefix(&subdir) {
                Ok(t) => path.push(t),
                Err(e) => {
                    error!("Failed to build target dir path: {}", e);
                    continue;
                }
            }
            
            // Path is expected to a dir, strip filename
            path.pop();

            queue.push((file, path));
        }
    }

    // Process queue in chunks of x images resizing in parallel
    let mut stats = ResizeStats::new();

    for c in queue.chunks(4) {
        let chunk = c.to_vec();
        let process = chunk.iter()
            .map(|t|t.0.to_string_lossy().into())
            .collect::<Vec<String>>();

        info!("Proceed chunk of {} source images [{}]", chunk.len(), process.join(", "));

        let mut handles = JoinSet::new();

        for (file, target) in chunk {
            let s3 = obj_store.clone();
            
            handles.spawn(async move {
                img_sizer(file, target, s3).await
            });
        }

        while let Some(r) = handles.join_next().await {
            match r {
                Ok(r) => stats.extend(r),
                Err(e) => panic!("Failed to join concurrently running resizer tasks: {}", e),
            }
        }
    }

    println!("------------------------------------------");
    info!("Processed {} files", queue.len());

    // Report errors
    if ! stats.failed.is_empty() {
        error!("Resizer failed for {} files", stats.failed.len());
        
        for (i, e) in stats.failed.iter().enumerate() {
            error!("{}: {}", i, e);
        }
    }

    // Report ignored
    if ! stats.skipped.is_empty() {
        warn!("Resizer skipped {} images", stats.skipped.len());
        
        for (i, s) in stats.skipped.iter().enumerate() {
            warn!("{}: {}", i, s);
        }
    }

    // Report succeeded
    if ! stats.succeeded.is_empty() {
        let tot = queue.len() - stats.failed.len();

        info!(
            "Resized {} source images. Each source image was resized to {} \
            variants and {} thumbnails", tot, IMG_TARGET_RATIOS.len(),
            THUMBNAIL_TARGET_RATIOS.len()
        );

        info!("In total {} images were saved.", stats.succeeded.len());
    }
}
