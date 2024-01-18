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

use device::{Action, Device};
use fs2::FileExt;
use futures::future::join_all;
use tokio::{main, spawn, sync::Mutex, task};

mod ble_server;
mod devices;
mod http_server;
mod thread_sharing;
use thread_sharing::{SharedConfig, SharedRequest};

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
                Verbosity: String::from("some"),
            }));
            let shared_request = Arc::new(Mutex::new(SharedRequest::NoUpdate));

            // Get the list of connected devices if applicable
            let mut located_devices = HashMap::new();
            if sub_matches.get_flag("no-nodes") {
                println!("Skipping getting devices!");
            } else {
                while located_devices.len() < devices::DEVICES.len()
                    && !shutdown_flag.load(Ordering::SeqCst)
                {
                    println!("Getting devices!!");
                    thread::sleep(time::Duration::from_millis(10000));
                    located_devices = devices::get_devices().await;
                }
            };
            println!("Devices:");
            for device in located_devices.keys() {
                println!("    {}", &device);
            }

            // Start the http server with the appropreate info passed in
            let shared_config_clone = shared_config.clone();
            let shared_request_clone = shared_request.clone();
            tokio::spawn(async move {
                http_server::run_http_server(shared_config_clone, shared_request_clone).await
            });
            println!("Http server started");

            // Start the bluetooth server
            let shared_request_clone = shared_request.clone();
            tokio::spawn(async move { ble_server::run_ble_server(shared_request_clone).await });

            println!("Ble server started");
            //while !shutdown_flag.load(Ordering::SeqCst) {
            //  tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            //}
            // Handle commands passed along from the server
            business_logic(
                located_devices,
                shutdown_flag.clone(),
                shared_request.clone(),
            )
            .await;
            println!("Shutdown!!!!!!!!");
            process::exit(0);
        }
        Some((SHUTDOWN_COMMAND, sub_matches)) => {
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
    located_devices: HashMap<String, devices::LocatedDevice>,
    shutdown_flag: Arc<AtomicBool>,
    shared_request_clone: Arc<Mutex<SharedRequest>>,
) {
    let mut last_device = (String::new(), Action::On, String::new());
    while !shutdown_flag.load(Ordering::SeqCst) {
        {
            let mut shared_request = shared_request_clone.lock().await;
            match &*shared_request {
                SharedRequest::Command {
                    ref device,
                    ref action,
                    ref target,
                } => {
                    let target = match target {
                        Some(t) => t.to_string(),
                        None => String::new(),
                    };
                    if last_device != (device.clone(), action.clone(), target.clone()) {
                        last_device = (device.clone(), action.clone(), target.clone());
                        dbg! {&last_device};
                        let located_device = located_devices.get(&device.clone());
                        dbg! {&located_device};
                        match located_device {
                            Some(d) => {
                                // println!("Command received!!! {}, {}, {}");
                                let url = format!(
                                    "http://{}/command?device={}&action={}&target={}",
                                    &d.ip,
                                    &device,
                                    &action.to_str().to_string(),
                                    &target
                                );
                                dbg!(&url);
                                reqwest::get(&url).await.unwrap();
                                *shared_request = SharedRequest::NoUpdate;
                            }
                            None => {
                                //println!("no device found");
                            }
                        }
                    }
                }
                SharedRequest::SliderInquiry => {
                    let mut futures = Vec::new();

                    for ld in located_devices.values() {
                        //let mut future = task::spawn(devices::get_device_status(&ld.ip, &ld.device.name));
                        let future =
                            get_device_status_helper(ld.ip.clone(), ld.device.name.clone());
                        futures.push(future);
                    }

                    let results = join_all(futures).await;
                    let mut result2 = HashMap::new();
                    for item in results {
                        let one = item.clone();
                        let two = item.clone();
                        result2.insert(one.unwrap().name, two.unwrap().target);
                    }
                    let response = format!(
                        "{}{}",
                        result2.get("kitchen light").unwrap().to_string(),
                        result2.get("bedroom light").unwrap().to_string()
                    );
                    dbg! {&response};
                    *shared_request = SharedRequest::SliderResponse { response: response };
                }
                SharedRequest::SliderResponse { response } => {}
                SharedRequest::NoUpdate => {
                    // println!("6666666666666 NoUpdate!!!");
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
}

async fn get_device_status_helper(ip: String, name: String) -> Result<Device, String> {
    devices::get_device_status(&ip, &name).await
}
