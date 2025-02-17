use gateway_server::GatewayServer;
use ic_agent::identity::BasicIdentity;
use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{filter, prelude::*};

use std::{
    fs::{self, File},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use structopt::StructOpt;

mod canister_methods;
mod canister_poller;
mod client_connection_handler;
mod gateway_server;
mod unit_tests;

#[derive(Debug, StructOpt)]
#[structopt(name = "Gateway", about = "IC WS Gateway")]
struct DeploymentInfo {
    #[structopt(short, long, default_value = "http://127.0.0.1:4943")]
    subnet_url: String,

    #[structopt(short, long, default_value = "0.0.0.0:8080")]
    gateway_address: String,

    #[structopt(short, long, default_value = "200")]
    polling_interval: u64,
}

fn load_key_pair() -> ring::signature::Ed25519KeyPair {
    if !Path::new("./data/key_pair").is_file() {
        let rng = ring::rand::SystemRandom::new();
        let key_pair = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng)
            .expect("Could not generate a key pair.");
        // TODO: print out seed phrase
        fs::write("./data/key_pair", key_pair.as_ref()).unwrap();
        ring::signature::Ed25519KeyPair::from_pkcs8(key_pair.as_ref())
            .expect("Could not read the key pair.")
    } else {
        let key_pair = fs::read("./data/key_pair").unwrap();
        ring::signature::Ed25519KeyPair::from_pkcs8(&key_pair)
            .expect("Could not read the key pair.")
    }
}

fn init_tracing() -> (WorkerGuard, WorkerGuard) {
    if !Path::new("./data/traces").is_dir() {
        fs::create_dir("./data/traces").unwrap();
    }

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let filename = format!("./data/traces/gateway_{:?}.log", timestamp.as_millis());

    println!("Tracing to file: {}", filename);

    let log_file = File::create(filename).expect("could not create file");
    let (non_blocking_file, guard_file) = tracing_appender::non_blocking(log_file);
    let (non_blocking_stdout, guard_stdout) = tracing_appender::non_blocking(std::io::stdout());
    let debug_log_file = tracing_subscriber::fmt::layer().with_writer(non_blocking_file);
    let debug_log_stdout = tracing_subscriber::fmt::layer().with_writer(non_blocking_stdout);
    tracing_subscriber::registry()
        .with(debug_log_file.with_filter(filter::LevelFilter::INFO))
        .with(debug_log_stdout.with_filter(filter::LevelFilter::INFO))
        .init();

    (guard_file, guard_stdout)
}

fn create_data_dir() {
    if !Path::new("./data").is_dir() {
        fs::create_dir("./data").unwrap();
    }
}

#[tokio::main]
async fn main() {
    create_data_dir();
    let _guards = init_tracing();

    let deployment_info = DeploymentInfo::from_args();
    info!("Deployment info: {:?}", deployment_info);

    let key_pair = load_key_pair();
    let identity = BasicIdentity::from_key_pair(key_pair);

    let mut gateway_server = GatewayServer::new(
        &deployment_info.gateway_address,
        &deployment_info.subnet_url,
        identity,
    )
    .await;

    // spawn a task which keeps accepting incoming connection requests from WebSocket clients
    gateway_server.start_accepting_incoming_connections();

    // maintains the WS Gateway state of the main task in sync with the spawned tasks
    gateway_server
        .manage_state(deployment_info.polling_interval)
        .await;
}
