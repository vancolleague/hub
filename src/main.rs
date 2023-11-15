use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::{thread, time};

mod devices;
mod server;
mod shared_request;
use shared_request::SharedRequest;

#[tokio::main]
async fn main() {
    let shared_request = Arc::new(Mutex::new(SharedRequest::NoUpdate));

    let mut devices = HashMap::new();
    while devices.len() < devices::DEVICES.len() {
        devices = devices::get_devices().await;
        thread::sleep(time::Duration::from_millis(10000));
    }
    println!("Devices:");
    for device in devices.keys() {
        println!("    {}", &device);
    }

    let shared_request_clone = shared_request.clone();
    thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(server::run_server(shared_request_clone))
    });

    let shared_request_clone = shared_request.clone();
    loop {
        {
            let mut shared_request = shared_request_clone.lock().unwrap();
            match *shared_request {
                SharedRequest::Command {
                    ref device,
                    ref action,
                    ref target,
                } => {
                    let target = match target {
                        Some(t) => t,
                        None => "",
                    };
                    let located_device = devices.get(&device.clone()).unwrap();
                    let url = format!(
                        "http://{}/command?device={}&action={}&target={}",
                        &located_device.ip,
                        &device,
                        &action.to_str().to_string(),
                        &target
                    );
                    *shared_request = SharedRequest::NoUpdate;
                }
                SharedRequest::NoUpdate => {
                    // println!("6666666666666 NoUpdate!!!");
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}
