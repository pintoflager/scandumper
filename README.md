# Image resizer and server

Rust project that compiles to 2 separate binaries. Runs on linux.

To give it a spin first [install minio](https://min.io/docs/minio/linux/operations/install-deploy-manage/deploy-minio-single-node-single-drive.html#minio-snsd) on your localhost.

To try the example you have to have minio instance up and running
```bash
minio server example/minio --address localhost:9090
```

If above command is executed within the project dir, minio database is stored inside the example dir. Only the port is important really.

Updater bin should be executed once before server bin is booted.

## Updater
Swallows a directories.

Jumps in looking for image files.

Tries to resize each found image into 4 different sizes.

Saves images to minio object store running locally.

Uses image files' relative path as a key.


Running the updater from a separate terminal tab or window:

```bash
cargo run --bin updater example
```

After command terminates minio's web console should display resized images.

## Server
Reads minio.

Returns json or image content.

Indexes, ask `http://127.0.0.1:9080/s3/index/root` and expect
```json
{"status":"200 OK","data":["root/one/","root/three/"]}
```

Objects, ask `http://127.0.0.1:9080/s3/list/root/one/two/apartments-1845884_640` and expect
```json
{"status":"200 OK","data":["root/one/two/apartments-1845884_640/lg.jpeg","root/one/two/apartments-1845884_640/md.jpeg","root/one/two/apartments-1845884_640/sm.jpeg","root/one/two/apartments-1845884_640/xs.jpeg"]}
```

Displays images with right content-type header, ask `http://127.0.0.1:9080/s3/get/root/one/two/apartments-1845884_640/md.jpeg` and expect an image to pop up on your browser window.

Running the server from a separate terminal tab or window:

```bash
cargo run --bin server example
```
