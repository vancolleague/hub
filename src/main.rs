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
use std::sync::{Arc, Mutex};
use std::{thread, time};

use fs2::FileExt;

mod devices;
mod server;
mod shared;
use shared::{SharedConfig, SharedRequest};

const EXIT_COMMAND: &str = "exit";
const LISTEN_ADDR: &str = "127.0.0.1:4000"; // Choose an appropriate address and port

fn handle_client(mut stream: TcpStream, exit_flag: Arc<AtomicBool>) {
    let mut buffer = [0; 1024];
    match stream.read(&mut buffer) {
        Ok(size) => {
            let received = String::from_utf8_lossy(&buffer[..size]);
            if received.trim() == EXIT_COMMAND {
                exit_flag.store(true, Ordering::SeqCst);
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
                        .help("Special behavior without nodes"),
                ),
            // ... additional settings or arguments specific to "run" ...
        )
        .subcommand(
            Command::new("exit").about("Exit's the program and it's it all down"), // ... additional settings or arguments specific to "run" ...
        )
        // run
        // exit
        // restart
        // get devices
        /*        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Sets a custom config file")
                .takes_value(true),
        )
        .arg(
            Arg::new("INPUT")
                .help("Sets the input file to use")
                .required(true)
                .index(1),
        ) */
        .arg(
            Arg::new("log_level")
                .long("log-level")
                .value_name("LEVEL")
                .help("Sets the level of logging"), //                .get_possible_values(["error", "warn", "info", "debug"])
                                                    //                .takes_value(true),
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
                // .multiple_occurrences(true)
                .help("Increases verbosity of output"),
        )
        .get_matches();

    match command.subcommand() {
        Some(("run", sub_matches)) => {
            let exit_flag = Arc::new(AtomicBool::new(false));
            let listener = TcpListener::bind(LISTEN_ADDR).expect("Failed to bind to address");

            // Get the current working directory
            let current_dir = match env::current_dir() {
                Ok(dir) => dir,
                Err(e) => {
                    eprintln!("Failed to determine current directory: {}", e);
                    process::exit(1);
                }
            };

            // Construct the path to the lock file
            let lock_path = current_dir.join("your_app.lock");

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

            // Spawn a thread to handle the TCP server
            let exit_flag_clone = Arc::clone(&exit_flag);
            thread::spawn(move || {
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            let exit_flag_clone = Arc::clone(&exit_flag_clone);
                            thread::spawn(move || {
                                handle_client(stream, exit_flag_clone);
                            });
                        }
                        Err(e) => eprintln!("Connection failed: {}", e),
                    }
                }
            });

            let shared_config = Arc::new(Mutex::new(SharedConfig {
                Verbosity: String::from("some"),
            }));
            let shared_request = Arc::new(Mutex::new(SharedRequest::NoUpdate));

            let mut devices = HashMap::new();
            if sub_matches.get_flag("no-nodes") {
                println!("Skipping getting devices!");
            } else {
                while devices.len() < devices::DEVICES.len() && !exit_flag.load(Ordering::SeqCst) {
                    println!("Getting devices!!");
                    thread::sleep(time::Duration::from_millis(10000));
                    devices = devices::get_devices().await;
                }
            };

            println!("Devices:");
            for device in devices.keys() {
                println!("    {}", &device);
            }

            let shared_config_clone = shared_config.clone();
            let shared_request_clone = shared_request.clone();
            thread::spawn(move || {
                let runtime = tokio::runtime::Runtime::new().unwrap();
                runtime.block_on(server::run_server(
                    shared_config_clone,
                    shared_request_clone,
                ))
            });

            let shared_request_clone = shared_request.clone();
            while !exit_flag.load(Ordering::SeqCst) {
                {
                    let mut shared_request = shared_request_clone.lock().unwrap();
                    match *shared_request {
                        SharedRequest::Command {
                            ref device,
                            ref action,
                            ref target,
                        } => {
                            let target = match target {
                                Some(t) => t.to_string(),
                                None => String::new(),
                            };
                            let located_device = devices.get(&device.clone()).unwrap();
                            let url = format!(
                                "http://{}/command?device={}&action={}&target={}",
                                &located_device.ip,
                                &device,
                                &action.to_str().to_string(),
                                &target
                            );
                            dbg!(&url);
                            reqwest::get(&url).await.unwrap();
                            *shared_request = SharedRequest::NoUpdate;
                        }
                        SharedRequest::NoUpdate => {
                            // println!("6666666666666 NoUpdate!!!");
                        }
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            }
            println!("Exited!!!!!!!!");
            process::exit(0);
        }
        Some(("exit", sub_matches)) => {
            println!("Exiting the program!!!");
            let mut stream = TcpStream::connect("127.0.0.1:4000").unwrap();
            stream.write_all(b"exit").unwrap();
            process::exit(0);
        }
        _ => {
            process::exit(1);
        }
    }
}
