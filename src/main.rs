use clap::Command;
use clap::{arg, Arg, ArgAction};
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{thread, time};

use bluer::Uuid;
use device::{Action, Device};
use fs2::FileExt;
use futures::future::join_all;
use tokio::{
    main, spawn,
    sync::{mpsc, Mutex},
    task,
};

mod ble_server;
mod devices;
mod http_server;
mod thread_sharing;
use thread_sharing::{SharedBLEAction, SharedConfig, SharedGetRequest};

const SHUTDOWN_COMMAND: &str = "shutdown";
const LISTEN_ADDR: &str = "127.0.0.1:4000"; // Choose an appropriate address and port

// Flag when the stream consists of the shutdown command
async fn handle_client(mut stream: TcpStream, shutdown_flag: Arc<AtomicBool>) {
    let mut buffer = [0; 1024];
    match stream.read(&mut buffer) {
        Ok(size) => {
            let received = String::from_utf8_lossy(&buffer[..size]);
            if received.trim() == SHUTDOWN_COMMAND {
                shutdown_flag.store(true, Ordering::SeqCst);
            }
        }
        Err(e) => eprintln!("Failed to receive data: {}", e),
    }
}

#[tokio::main]
async fn main() {
    let command = Command::new("Hub")
        .version("0.1")
        .author("Chad DeRosier, <chad.derosier@tutanota.com>")
        .about("Runs van automation stuff.")
        .subcommand(
            Command::new("run")
                .about("Runs the application") // ... additional settings or arguments specific to "run" ...
                .arg(
                    Arg::new("no-nodes")
                        .short('n')
                        .long("no-nodes")
                        .action(clap::ArgAction::SetTrue)
                        .help("Run without waiting to find all of the expected nodes"),
                )
                .arg(
                    Arg::new("node-count")
                        .short('c')
                        .long("node-count")
                        .action(clap::ArgAction::Set)
                        .help("Set the number of nodes to look for."),
                ),
        )
        .subcommand(
            Command::new("shutdown").about("Shutdown's the program and it's it all down"), // ... additional settings or arguments specific to "run" ...
        )
        .arg(
            Arg::new("log_level")
                .long("log-level")
                .value_name("LEVEL")
                .help("Sets the level of logging"),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .help("Silences most output"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Increases verbosity of output"),
        )
        .get_matches();

    match command.subcommand() {
        Some(("run", sub_matches)) => {
            // Spawn a thread to handle the TCP server that's userd for sending/receiving
            // commands from other consoles
            let shutdown_flag = Arc::new(AtomicBool::new(false));
            let listener = TcpListener::bind(LISTEN_ADDR).expect("Failed to bind to address");
            let shutdown_flag_clone = Arc::clone(&shutdown_flag);
            tokio::spawn(async move {
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            let shutdown_flag_clone = Arc::clone(&shutdown_flag_clone);
                            handle_client(stream, shutdown_flag_clone).await;
                        }
                        Err(e) => eprintln!("Connection failed: {}", e),
                    }
                }
            });

            // Get the current working directory
            let current_dir = match env::current_dir() {
                Ok(dir) => dir,
                Err(e) => {
                    eprintln!("Failed to determine current directory: {}", e);
                    process::exit(1);
                }
            };

            // Construct the path to the lock file
            let lock_path = current_dir.join("hub_app.lock");
            let file = match File::create(&lock_path) {
                Ok(file) => file,
                Err(e) => {
                    eprintln!("Failed to create lock file: {}", e);
                    process::exit(1);
                }
            };

            // Try to acquire an exclusive lock
            if file.try_lock_exclusive().is_err() {
                eprintln!("Another instance of the application is already running.");
                process::exit(1);
            }

            // Set up stuff that needs to be shared between threads
            let shared_config = Arc::new(Mutex::new(SharedConfig {
                verbosity: String::from("some"),
            }));
            let shared_request = Arc::new(Mutex::new(SharedGetRequest::NoUpdate));

            // Get the list of connected devices if applicable
            let mut located_devices = HashMap::new();
            let node_count: Option<&String> = sub_matches.get_one("node-count");
            if sub_matches.get_flag("no-nodes") {
                println!("Skipping getting devices!");
            } else {
                println!("Getting Devices!!");
                match node_count { 
                    Some(nc) => {
                        let nc: usize = nc.parse().unwrap();
                        while located_devices.len() < nc && !shutdown_flag.load(Ordering::SeqCst) {
                            thread::sleep(time::Duration::from_millis(10000));
                            located_devices = devices::get_devices().await;
                        }
                    }
                    None => {
                        thread::sleep(time::Duration::from_millis(10000));
                        located_devices = devices::get_devices().await;
                    }
                }
            }

            println!("Devices:");
            for device in located_devices.keys() {
                println!("    {}", &device);
            }

            // Start the http server with the appropreate info passed in
            let shared_config_clone = shared_config.clone();
            let shared_request_clone = shared_request.clone();
            let devices = located_devices.iter().map(|(u, ld)| (ld.device.name.clone(), u.clone())).collect::<Vec<(String, Uuid)>>();
            tokio::spawn(async move {
                http_server::run_http_server(shared_config_clone, shared_request_clone, devices).await
            });
            println!("Http server started");

            // Start the bluetooth server
            let shared_ble_action = Arc::new(Mutex::new(thread_sharing::SharedBLEAction::NoUpdate));
            let shared_ble_action_clone = shared_ble_action.clone();
            let devices: Vec<(String, Uuid)> = located_devices
                .iter()
                .map(|(_, ld)| (ld.device.name.clone(), ld.device.uuid.clone()))
                .collect();
            tokio::spawn(async move { ble_server::run_ble_server(shared_ble_action_clone, devices).await });

            println!("Ble server started");
            business_logic(
                located_devices,
                shutdown_flag.clone(),
                shared_request.clone(),
                shared_ble_action.clone(),
            )
            .await;
            println!("Shutdown!!!!!!!!");
            process::exit(0);
        }
        Some((SHUTDOWN_COMMAND, _sub_matches)) => {
            println!("Shutting down the program!!!");
            let mut stream = TcpStream::connect("127.0.0.1:4000").unwrap();
            stream.write_all(SHUTDOWN_COMMAND.as_bytes()).unwrap();
            process::exit(0);
        }
        _ => {
            println!("You must enter a command, perhapse you wanted:");
            println!("  > hub run");
            println!("or");
            println!("  > hub help");
            process::exit(1);
        }
    }
}

async fn business_logic(
    mut located_devices: HashMap<Uuid, devices::LocatedDevice>,
    shutdown_flag: Arc<AtomicBool>,
    shared_get_request: Arc<Mutex<SharedGetRequest>>,
    shared_ble_action: Arc<Mutex<SharedBLEAction>>,
) {
    let mut last_action = (Uuid::from_u128(0x0), Action::On);
    while !shutdown_flag.load(Ordering::SeqCst) {
        {
            use SharedGetRequest::*;
            let mut shared_request = shared_get_request.lock().await;
            match &*shared_request {
                Command {
                    ref device_uuid,
                    ref action,
                } => {
                    if last_action != (device_uuid.clone(), action.clone()) {
                        last_action = (device_uuid.clone(), action.clone());
                        dbg! {&last_action};
                        let located_device = located_devices.get(&device_uuid);
                        dbg! {&located_device};
                        let target = match action.get_target() {
                                Some(t) => t.to_string(),
                                None => "".to_string()
                            };
                        match located_device {
                            Some(d) => {
                                // println!("Command received!!! {}, {}, {}");
                                let url = format!(
                                    "http://{}/command?uuid={}&action={}&target={}",
                                    &d.ip,
                                    &device_uuid,
                                    &action.to_str().to_string(),
                                    &target
                                );
                                dbg!(&url);
                                reqwest::get(&url).await.unwrap();
                                *shared_request = SharedGetRequest::NoUpdate;
                            }
                            None => {
                                //println!("no device found");
                            }
                        }
                    }
                }
                NoUpdate => {
                    // println!("6666666666666 NoUpdate!!!");
                }
            }
        }
        {
            use SharedBLEAction::*;
            let mut shared_action = shared_ble_action.lock().await;
            match &*shared_action {
                Command {
                    ref device_uuid,
                    ref action,
                } => {
                    let located_device = located_devices.get_mut(&device_uuid).unwrap();
                    let _ = located_device.device.take_action(action.clone());
                    *shared_action = NoUpdate;
                },
                TargetInquiry {
                    ref device_uuid,
                } => {
                    let located_device = located_devices.get(&device_uuid).unwrap();
                    let device = get_device_status_helper(located_device.ip.clone(), device_uuid.clone()).await; 
                    *shared_action = TargetResponse {
                            target: device.unwrap().target.clone()
                        };
                },
                TargetResponse { .. } => {},
                NoUpdate => {},
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
}

async fn get_device_status_helper(ip: String, uuid: Uuid) -> Result<Device, String> {
    devices::get_device_status(&ip, &uuid).await
}
