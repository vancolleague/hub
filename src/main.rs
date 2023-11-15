use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::{thread, time};

mod devices;
mod server;
mod shared_request;
use shared_request::SharedRequest;

#[tokio::main]
async fn main() {
    let shared_request = Arc::new(Mutex::new(SharedRequest {
        device: "".to_string(),
        ip: "".to_string(),
        uri: "".to_string(),
        updated: false,
    }));

    //    let device_names = Vec::from(["bedroom light".to_string(), "kitchen light".to_string()]);

    let mut devices = HashMap::new();
    while devices.len() < devices::device_names.len() {
        devices = devices::get_devices().await;
        thread::sleep(time::Duration::from_millis(10000));
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
            if shared_request.updated == true {
                println!("```````````````````````````````````   Updated!");
                shared_request.updated = false;
            }
            for (n, d) in devices.iter() {
                let device = devices::get_device_status(&d.ip, &d.device.name).await;
                //                dbg!(device);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(5000)).await;
        //thread::sleep(time::Duration::from_millis(5000));
    }
}
