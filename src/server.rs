use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use actix_web::{middleware, web, App, HttpRequest, HttpResponse, HttpServer};

use device::{Action, Device};
//mod crate::shared_request;
use crate::devices;
use crate::shared_request::SharedRequest;

async fn index(
    req: HttpRequest,
    shared_request_clone: web::Data<Arc<Mutex<SharedRequest>>>,
) -> &'static str {
    println!("REQ: {req:?}");
    {
        let mut shared_request = shared_request_clone.lock().unwrap();
        shared_request.updated = true;
    }
    "Nothing here!"
}

// Expect to be http://ip:8080?device=device%20name&action=act&target=tar
async fn command(
    req: HttpRequest,
    info: web::Query<HashMap<String, String>>,
    shared_request_clone: web::Data<Arc<Mutex<SharedRequest>>>,
) -> HttpResponse {
    let device = match info.get("device") {
        Some(d) => d,
        None => return HttpResponse::Ok().body("Oops, we didn't get the Device")
    };

    let action = match info.get("action") {
        Some(a) => a,
        None => return HttpResponse::Ok().body("Oops, we didn't get the Action")
    };
    let action = match Action::from_str(action.as_str()) {
        Ok(a) => a,
        Err(_) => return HttpResponse::Ok().body("Oops, Action was invalid")
    };

    let target: Option<usize> = match info.get("target") {
        Some(t) => {
            if t.as_str() != "" {
                match t.parse::<usize>() {
                    Ok(n) => {
                        if 0 <= n && n < 8 {
                            Some(n) 
                        } else {
                            return HttpResponse::Ok().body("Oops, Target should be 0 <= t < 8") 
                        }
                    },
                    Err(_) => return HttpResponse::Ok().body("Oops, Target should be 0 <= t < 8"), 
                }
            } else {
                None
            }
        },
        None => None
    };
    dbg!(&target);

    let result = "place holder".to_string();
    HttpResponse::Ok().body(result)
}

pub async fn run_server(shared_request_clone: Arc<Mutex<SharedRequest>>) -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    log::info!("starting HTTP server at http://localhost:8080");

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .data(shared_request_clone.clone())
            //          .service(web::resource("/index.html").to(|| async { "Hello world!" }))
            //            .service(web::resource("/instruction").to(instruction))
            .service(web::resource("/").to(index))
            .service(web::resource("/command").to(command))
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
