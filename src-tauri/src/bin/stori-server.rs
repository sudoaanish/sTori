#[path = "../db.rs"]
mod db;
#[path = "../downloads.rs"]
mod downloads;
#[path = "../error.rs"]
mod error;
#[path = "../models.rs"]
mod models;
#[path = "../scanner.rs"]
mod scanner;
#[path = "../server.rs"]
mod server;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("stori=info,tower_http=info")
        .init();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let data = root.join(".stori-dev");
    std::fs::create_dir_all(&data).expect("create development data directory");
    let database = db::Database::open(&data.join("stori.db")).expect("open development database");
    let managed_library = dirs::download_dir()
        .unwrap_or_else(|| root.join("Downloads"))
        .join("sTori Books");
    let listener = std::net::TcpListener::bind(("0.0.0.0", server::PORT))
        .expect("bind sTori development server");
    server::run_with_std_listener(
        server::ServerState::new(database, managed_library, data.join("cover-cache")),
        root.join("dist"),
        listener,
    )
    .await
    .expect("run sTori server");
}
