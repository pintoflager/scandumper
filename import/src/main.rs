mod resize;
mod transform;
mod actions;

use std::fs::read_dir;
use tokio::task::JoinSet;
use tracing::{error, debug, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use walkdir::WalkDir;

use config::*;
use resize::*;
use actions::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "scandumper=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    debug!("Logging initialized...");

    let mut config = match Config::new() {
        Ok(c) => c,
        Err(e) => panic!("Failed to init config: {}", e),
    };

    debug!("Config loaded...");

    let export_config = match config.export() {
        Ok(e) => e,
        Err(e) => panic!("Incomplete config for export: {}", e),
    };

    debug!("Export config loaded...");

    let import_config = match config.import() {
        Ok(i) => i,
        Err(e) => panic!("Incomplete config for import: {}", e),
    };

    debug!("Import config loaded...");

    // Init object store from config
    let s3_store = match export_config.s3 {
        true => {
            let s3_config = match config.s3_mut() {
                Ok(c) => c,
                Err(e) => panic!("Incomplete config for S3: {}", e),
            };

            // Verify that our bucket exists
            if let Err(e) = ObjectStore::init_from(s3_config, true).await {
                panic!("Object store setup from config failed: {}", e)
            }
            Some(ObjectStore::get(&s3_config).unwrap())
        },
        false => None,
    };

    // Check if filesystem export config is valid
    if export_config.filesystem {
        if export_config.filesystem_path.is_none() && export_config.prefix.is_none() {
            panic!("Filesystem export requires either 'filesystem_path' or 'prefix' to be set");
        }
    }

    let mut queue = vec![];

    if import_config.include.is_none() {
        debug!("No limited set of subdirs specified, using config.toml root dir as source");
    }

    // Read all files from config.toml dir
    let mut root_dirs = vec![];

    for e in read_dir(&config.dir).expect("Failed to read config.toml parent dir") {
        let entry = match e {
            Ok(i) => i,
            Err(e) => {
                error!("Failed to read config.toml dir entry: {}", e);
                continue;
            }
        };

        let path = entry.path();

        if export_config.filesystem {
            // If we're exporting to filesystem and target path is given outside the config dir
            // we can skip the prefix comparison
            if let Some(ref i) = export_config.filesystem_path {
                if config.dir.ne(i) {
                    continue;
                }
            }

            // Next iteration should not read exported files again. Compare prefix.
            if let Some(ref i) = export_config.prefix {
                if path.ends_with(i) {
                    debug!("Skipping filesystem path as it ends to export prefix: {:?}", path);

                    continue;
                }
            }
        }

        if path.is_dir() {
            match import_config.include {
                Some(ref v) => match v.iter().any(|t|path.ends_with(t)) {
                    true => root_dirs.push(path),
                    false => warn!("Skipping dir import as it's not in limited set: {:?}", path),
                },
                None => root_dirs.push(path),
            }
        }
    }

    match root_dirs.is_empty() {
        true => panic!("No iterable directories found next to config.toml file"),
        false => debug!("Iterating image ({}) rootdir(s), preparing resizer queue...", root_dirs.len()),
    }

    // Iterate non-exluded subdirs from config.toml dir
    for p in root_dirs.iter() {
        // Walk all files in subdir
        for i in WalkDir::new(&p) {
            let e = match i {
                Ok(x) => x,
                Err(e) => {
                    error!("Failed to read dir entry: {}", e);
                    continue;
                }
            };

            let file = match e.path().is_file() {
                true => e.into_path(),
                false => continue,
            };

            info!("Sending file {:?} to resizer queue", &file);
            let mut target_dir = config.dir.to_owned();

            // Target dir starts with a custom root dir?
            if let Some(ref e) = export_config.prefix {
                target_dir.push(e);
            };

            // Add root dir to target path
            target_dir.push(p.file_name().expect("Failed to extract root dir name"));

            // Add subpath of file into the target path
            match file.strip_prefix(&p) {
                Ok(t) => target_dir.push(t),
                Err(e) => {
                    error!("Failed to build target dir path: {}", e);
                    continue;
                }
            }
            
            // Path is expected to a dir, strip filename
            target_dir.pop();

            queue.push((file, target_dir));
        }
    }

    // Should we save resized images to filesystem?
    let export_fs = match export_config.filesystem {
        true => match export_config.filesystem_path {
            Some(ref p) => Some(p.to_owned()),
            None => Some(config.dir.to_owned()),
        },
        false => None,
    };

    // Process queue concurrently
    let queue_len = queue.len();
    let mut stats = ResizeStats::new();
    
    // Lower chunk size for slower machines. Parallel processing of multiple large images
    // is a heavy task and can drain all resources.
    let chunk_size = match config.parallel_img_max {
        Some(u) => u,
        None => 4,
    };

    info!("Processing resize queue of {} files in chunks of {} images concurrently...", queue_len, chunk_size);

    for c in queue.chunks(chunk_size) {
        let chunk = c.to_vec();
        let process = chunk.iter()
            .map(|t|t.0.to_string_lossy().into())
            .collect::<Vec<String>>();

        debug!("Proceed to resizing chunk of {} source images [{}]", chunk.len(), process.join(", "));

        let mut handles = JoinSet::new();

        for (importable, target) in chunk {
            let s3 = s3_store.clone();
            let fs = export_fs.clone();
            let conf = config.clone();
            
            handles.spawn(async move {
                resize_action(importable, target, conf, fs, s3).await
            });
        }

        while let Some(r) = handles.join_next().await {
            match r {
                Ok(r) => stats.extend(r),
                Err(e) => panic!("Failed to join concurrently running resizer tasks: {}", e),
            }
        }
    }

    // Run transformations against resized images to save up some resources
    if let Some(f) = TargetSize::transform_size_variant(&config) {
        info!("Processing transform queue of {} files in chunks of {} images concurrently...", queue_len, chunk_size);
    
        let source_img_opts = [
            format!("{}.jpeg", f.to_str()),
            format!("{}.png", f.to_str()),
        ];
    
        for c in queue.chunks(chunk_size) {
            let chunk = c.to_vec();
            debug!("Proceed to transforming chunk of {} resized source images", chunk.len());
    
            let mut handles = JoinSet::new();
    
            for (filepath, mut target) in chunk {
                // Take filename without the extension from te filepath
                let filename = filepath.file_stem().expect("Failed to extract filename from path");

                // Try both possible resized image files, other should exist
                for o in source_img_opts.iter() {
                    let mut importable = target.clone();
                    importable.push(filename);
                    importable.push(o);

                    if importable.is_file() {
                        let s3 = s3_store.clone();
                        let fs = export_fs.clone();
                        let size = f.clone();
                        target.push(filename);

                        handles.spawn(async move {
                            transform_action(importable, target, size, fs, s3).await
                        });

                        break;
                    }
                    else {
                        debug!("Skipping transform for {:?} as it doesn't exist", importable);
                    }
                }
            }
    
            while let Some(r) = handles.join_next().await {
                match r {
                    Ok(r) => stats.extend(r),
                    Err(e) => panic!("Failed to join concurrently running resizer tasks: {}", e),
                }
            }
        }
    }

    // for (importable, target) in queue {
    //     let s3 = s3_store.clone();
    //     let fs = export_fs.clone();
        
    //     handles.spawn(async move {
    //         img_sizer(importable, target, fs, s3).await
    //     });
    // }

    // while let Some(r) = handles.join_next().await {
    //     match r {
    //         Ok(r) => stats.extend(r),
    //         Err(e) => error!("Failed to join concurrently running resizer tasks: {}", e),
    //     }
    // }

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
        let tot = queue_len - stats.failed.len();

        info!("Resized successfully {} source images", tot);
        info!("In total {} images were saved.", stats.succeeded.len());
    }
}
