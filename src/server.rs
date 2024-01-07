use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use actix_web::{middleware, web, App, HttpRequest, HttpResponse, HttpServer};

use device::{Action, Device};
//mod crate::shared_request;
use crate::devices::DEVICES;
use crate::shared::{SharedConfig, SharedRequest};

async fn index(
    req: HttpRequest,
    shared_config_clone: web::Data<Arc<Mutex<SharedConfig>>>,
    shared_request_clone: web::Data<Arc<Mutex<SharedRequest>>>,
) -> &'static str {
    println!("REQ: {req:?}");
    {
        let mut shared_request = shared_request_clone.lock().unwrap();
        *shared_request = SharedRequest::NoUpdate;
    }
    "Nothing here!"
}

// Expect to be http://ip:8080?device=device%20name&action=act&target=tar
async fn parsed_command(
    req: HttpRequest,
    info: web::Query<HashMap<String, String>>,
    shared_config_clone: web::Data<Arc<Mutex<SharedConfig>>>,
    shared_request_clone: web::Data<Arc<Mutex<SharedRequest>>>,
) -> HttpResponse {
    let device = match info.get("device") {
        Some(d) => d.to_lowercase(),
        None => return HttpResponse::Ok().body("Oops, we didn't get the Device"),
    };
    if !DEVICES.contains(&device.as_str()) {
        return HttpResponse::Ok().body("Oops, we didn't get the Device");
    }

    let action = match info.get("action") {
        Some(a) => a.to_lowercase(),
        None => return HttpResponse::Ok().body("Oops, we didn't get the Action"),
    };
    let action = match Action::from_str(action.as_str()) {
        Ok(a) => a,
        Err(_) => return HttpResponse::Ok().body("Oops, Action was invalid"),
    };

    let target: Option<usize> = match info.get("target") {
        Some(t) => {
            if t.as_str() != "" {
                match t.parse::<usize>() {
                    Ok(n) => {
                        if n < 8 {
                            Some(n)
                        } else {
                            return HttpResponse::Ok().body("Oops, Target should be 0 <= t < 8");
                        }
                    }
                    Err(_) => return HttpResponse::Ok().body("Oops, Target should be 0 <= t < 8"),
                }
            } else {
                None
            }
        }
        None => None,
    };

    let mut result = {
        let mut shared_request = shared_request_clone.lock().unwrap();
        *shared_request = SharedRequest::Command {
            device: device.to_string(),
            action: action,
            target: target,
        };
        serde_json::to_string(&(*shared_request)).unwrap()
    };
    HttpResponse::Ok().body(result)
}

async fn command(
    req: HttpRequest,
    info: web::Query<HashMap<String, String>>,
    shared_config_clone: web::Data<Arc<Mutex<SharedConfig>>>,
    shared_request_clone: web::Data<Arc<Mutex<SharedRequest>>>,
) -> HttpResponse {
    let instruction = match info.get("instruction") {
        Some(i) => i,
        None => return HttpResponse::Ok().body("Oops, we didn't get the instruction"),
    };

    dbg!(&info);
    let mut device = String::new();
    let instruction = instruction.replace("%20", " ").to_lowercase();
    let mut instruction = instruction.split_whitespace();
    while !DEVICES.contains(&device.as_str()) {
        let word = match instruction.next() {
            Some(w) => w,
            None => return HttpResponse::Ok().body("Oops, we didn't get a device!"),
        };

        if device.is_empty() {
            device = word.to_string();
        } else {
            device = format!("{} {}", &device, &word);
        }
    }

    let action = match instruction.next() {
        Some(a) => a,
        None => return HttpResponse::Ok().body("Oops, we didn't get an action!"),
    };
    let action = match Action::from_str(&action) {
        Ok(a) => a,
        Err(_) => return HttpResponse::Ok().body("Oops, the given action was invalid"),
    };

    let target = match instruction.next() {
        Some(t) => {
            if t.is_empty() {
                None
            } else {
                match t.parse::<usize>() {
                    Ok(n) => {
                        if n < 8 {
                            Some(n)
                        } else {
                            return HttpResponse::Ok().body("Oops, Target should be 0 <= t < 8");
                        }
                    }
                    Err(_) => {
                        return HttpResponse::Ok()
                            .body("Oops, Target should be a number, 0 though 7")
                    }
                }
            }
        }
        None => None,
    };

    let result = {
        let mut shared_request = shared_request_clone.lock().unwrap();
        *shared_request = SharedRequest::Command {
            device: device.to_string(),
            action: action,
            target: target,
        };
        serde_json::to_string(&(*shared_request)).unwrap()
    };
    HttpResponse::Ok().body(result)
}

pub async fn run_server(
    shared_config_clone: Arc<Mutex<SharedConfig>>,
    shared_request_clone: Arc<Mutex<SharedRequest>>,
) -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    log::info!("starting HTTP server at http://localhost:8080");

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .data(shared_config_clone.clone())
            .data(shared_request_clone.clone())
            .service(web::resource("/").to(index))
            .service(web::resource("/parsed_command").to(parsed_command))
            .service(web::resource("/command").to(command))
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
