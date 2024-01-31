//! Serves a Bluetooth GATT application using the callback programming model.
use std::str::FromStr;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

use bluer::{
    adv::Advertisement,
    gatt::local::{
        Application, Characteristic, CharacteristicNotify, CharacteristicNotifyMethod,
        CharacteristicRead, CharacteristicWrite, CharacteristicWriteMethod, Service,
    },
    Uuid,
};
use futures::FutureExt;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::{mpsc, Mutex},
    time::sleep,
};

use device::{Action, Device};

use crate::thread_sharing::*;

const KITCHEN_UUID: Uuid = Uuid::from_u128(0x36bc0fe1b00742809ec6b36c8bc98537);
const BEDROOM_UUID: Uuid = Uuid::from_u128(0x0584507902e74f44b67902b90775abda);
const VOICE_UUID: Uuid = Uuid::from_u128(0x7e1be1ebf9844e17b0f1049e02a39567);
const SET_UUID: Uuid = Uuid::from_u128(0x2a4fae8107134e1fa8187ac56e4f13e4);
const _ON_UUID: Uuid = Uuid::from_u128(0x928e9b929939486b998d69613f89a9a6);
#[allow(dead_code)]
const MANUFACTURER_ID: u16 = 0x45F1;

pub async fn run_ble_server(
    shared_action: Arc<Mutex<SharedBLEAction>>,
    devices: Vec<(String, Uuid)>,
) {
    let session = bluer::Session::new().await.unwrap();
    let adapter = session.default_adapter().await.unwrap();
    adapter.set_powered(true).await.unwrap();

    println!(
        "Advertising on Bluetooth adapter {} with address {}",
        adapter.name(),
        adapter.address().await.unwrap()
    );
    let mut manufacturer_data = BTreeMap::new();
    manufacturer_data.insert(MANUFACTURER_ID, vec![0x21, 0x22, 0x23, 0x24]);
    let le_advertisement = Advertisement {
        service_uuids: vec![BEDROOM_UUID].into_iter().collect(),
        manufacturer_data: manufacturer_data.clone(),
        discoverable: Some(true),
        local_name: Some("VanColleague".to_string()),
        ..Default::default()
    };
    let adv_handle = adapter.advertise(le_advertisement).await.unwrap();

    println!(
        "Serving GATT service on Bluetooth adapter {}",
        adapter.name()
    );
    let shared_kitchen_set_read = shared_action.clone();
    let shared_kitchen_set_write = shared_action.clone();
    let shared_kitchen_set_notify = shared_action.clone();
    let shared_voice_set_write = shared_action.clone();
    let shared_bedroom_set_read = shared_action.clone();
    let shared_bedroom_set_write = shared_action.clone();
    let value = Arc::new(Mutex::new(vec![0x10, 0x01, 0x01, 0x10]));
    let value_notify = value.clone();
    let value_notify2 = value.clone();
    let value_read2 = value.clone();
    let value_write2 = value.clone();
    let app = Application {
        services: vec![
            Service {
                uuid: BEDROOM_UUID,
                primary: true,
                characteristics: vec![Characteristic {
                    uuid: SET_UUID,
                    read: Some(CharacteristicRead {
                        read: true,
                        fun: Box::new(move |req| {
                            let value = value_read2.clone();
                            let shared_action_clone = shared_bedroom_set_read.clone();
                            async move {
                                {
                                    let mut shared_action_guard = shared_action_clone.lock().await;
                                    *shared_action_guard = SharedBLEAction::TargetInquiry {
                                        device_uuid: BEDROOM_UUID,
                                    };
                                }
                                let response =
                                    await_for_inquiry_response(shared_action_clone.clone()).await;
                                println!("BLE response: {}", &response);
                                Ok(response.to_string().as_bytes().to_vec())
                            }
                            .boxed()
                        }),
                        ..Default::default()
                    }),
                    write: Some(CharacteristicWrite {
                        write: true,
                        write_without_response: true,
                        method: CharacteristicWriteMethod::Fun(Box::new(move |new_value, req| {
                            let shared_action_clone = shared_bedroom_set_write.clone();
                            async move {
                                let text = std::str::from_utf8(&new_value).unwrap();
                                let target: usize =
                                    text.chars().take(1).collect::<String>().parse().unwrap();
                                {
                                    let mut shared_action_guard = shared_action_clone.lock().await;
                                    *shared_action_guard = SharedBLEAction::Command {
                                        device_uuid: BEDROOM_UUID,
                                        action: Action::Set { target: target },
                                    };
                                }
                                Ok(())
                            }
                            .boxed()
                        })),
                        ..Default::default()
                    }),
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        method: CharacteristicNotifyMethod::Fun(Box::new(move |mut notifier| {
                            let value = value_notify2.clone();
                            //  let _shared_action_clone = shared_kitchen_set_notify.clone();
                            async move {
                                tokio::spawn(async move {
                                    println!(
                                        "Notification session start with confirming={:?}",
                                        notifier.confirming()
                                    );
                                    loop {
                                        {
                                            let mut value = value.lock().await;
                                            println!("Notifying with value {:x?}", &*value);
                                            if let Err(err) = notifier.notify(value.to_vec()).await
                                            {
                                                println!("Notification error: {}", &err);
                                                break;
                                            }
                                            println!("Decrementing each element by one");
                                            for v in &mut *value {
                                                *v = v.saturating_sub(1);
                                            }
                                        }
                                        sleep(Duration::from_secs(5)).await;
                                    }
                                    println!("Notification session stop");
                                });
                            }
                            .boxed()
                        })),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            },
            Service {
                uuid: KITCHEN_UUID,
                primary: true,
                characteristics: vec![Characteristic {
                    uuid: SET_UUID,
                    read: Some(CharacteristicRead {
                        read: true,
                        fun: Box::new(move |req| {
                            dbg!(&req); // todo: does req have the uuid to look up the device?
                            let shared_action_clone = shared_kitchen_set_read.clone();
                            async move {
                                {
                                    let mut shared_action_guard = shared_action_clone.lock().await;
                                    *shared_action_guard = SharedBLEAction::TargetInquiry {
                                        device_uuid: KITCHEN_UUID,
                                    };
                                }
                                let response =
                                    await_for_inquiry_response(shared_action_clone.clone()).await;
                                println!("BLE response: {}", &response);
                                Ok(response.to_string().as_bytes().to_vec())
                            }
                            .boxed()
                        }),
                        ..Default::default()
                    }),
                    write: Some(CharacteristicWrite {
                        write: true,
                        write_without_response: true,
                        method: CharacteristicWriteMethod::Fun(Box::new(move |new_value, _req| {
                            let shared_action_clone = shared_kitchen_set_write.clone();
                            async move {
                                let text = std::str::from_utf8(&new_value).unwrap();
                                let target: usize =
                                    text.chars().take(1).collect::<String>().parse().unwrap();
                                {
                                    let mut shared_action_guard = shared_action_clone.lock().await;
                                    *shared_action_guard = SharedBLEAction::Command {
                                        device_uuid: KITCHEN_UUID,
                                        action: Action::Set { target: target },
                                    };
                                }
                                Ok(())
                            }
                            .boxed()
                        })),
                        ..Default::default()
                    }),
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        method: CharacteristicNotifyMethod::Fun(Box::new(move |mut notifier| {
                            let value = value_notify.clone();
                            let _shared_action_clone = shared_kitchen_set_notify.clone();
                            async move {
                                tokio::spawn(async move {
                                    println!(
                                        "Notification session start with confirming={:?}",
                                        notifier.confirming()
                                    );
                                    loop {
                                        {
                                            let mut value = value.lock().await;
                                            println!("Notifying with value {:x?}", &*value);
                                            if let Err(err) = notifier.notify(value.to_vec()).await
                                            {
                                                println!("Notification error: {}", &err);
                                                break;
                                            }
                                            println!("Decrementing each element by one");
                                            for v in &mut *value {
                                                *v = v.saturating_sub(1);
                                            }
                                        }
                                        sleep(Duration::from_secs(5)).await;
                                    }
                                    println!("Notification session stop");
                                });
                            }
                            .boxed()
                        })),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            },
            Service {
                uuid: VOICE_UUID,
                primary: true,
                characteristics: vec![Characteristic {
                    uuid: VOICE_UUID,
                    read: Some(CharacteristicRead {
                        read: false,
                        ..Default::default()
                    }),
                    write: Some(CharacteristicWrite {
                        write: true,
                        write_without_response: true,
                        method: CharacteristicWriteMethod::Fun(Box::new(move |new_value, req| {
                            let shared_action_clone = shared_voice_set_write.clone();
                            let devices_clone = devices.clone();
                            async move {
                                let command = std::str::from_utf8(&new_value).unwrap();
                                let command = command.to_lowercase();
                                let mut command = command.split_whitespace();

                                let mut device = String::new();

                                while devices_clone
                                    .clone()
                                    .iter()
                                    .map(|(n, _)| n)
                                    .collect::<Vec<&String>>()
                                    .contains(&&device)
                                {
                                    //while !DEVICES.contains_key(&device.as_str()) {
                                    let word = match command.next() {
                                        Some(w) => w,
                                        None => {
                                            //           return HttpResponse::Ok()
                                            //             .body("Oops, we didn't get a device!")
                                            panic!("Didn't get the device name");
                                        }
                                    };

                                    if device.is_empty() {
                                        device = word.to_string();
                                    } else {
                                        device = format!("{} {}", &device, &word);
                                    }
                                }

                                let mut uuid = Uuid::from_u128(0x0);
                                for (n, u) in devices_clone.iter() {
                                    if &device == n {
                                        uuid = u.clone();
                                        break;
                                    }
                                }
                                if uuid.as_u128() == 0x0 {
                                    //   return HttpResponse::Ok().body("Couldn't get the uuid of the named device.");
                                    panic!("didn't get the device id");
                                }

                                let action = match command.next() {
                                    Some(a) => a,
                                    None => panic!("failed to get an action"), //return HttpResponse::Ok().body("Oops, we didn't get an action!"),
                                };
                                let target = match command.next() {
                                    Some(t) => {
                                        if t.is_empty() {
                                            None
                                        } else {
                                            match t.parse::<usize>() {
                                                Ok(n) => {
                                                    if n < 8 {
                                                        Some(n)
                                                    } else {
                                                        panic!("Target's too high");
                                                        // return HttpResponse::Ok().body("Oops, Target should be 0 <= t < 8");
                                                    }
                                                }
                                                Err(_) => {
                                                    panic!("parse issue");
                                                    //return HttpResponse::Ok()
                                                    //  .body("Oops, Target should be a number, 0 though 7")
                                                }
                                            }
                                        }
                                    }
                                    None => None,
                                };

                                let action = match Action::from_str(action, target) {
                                    Ok(a) => a,
                                    Err(_) => {
                                        panic!("Issue creating the action");
                                        //                                        return HttpResponse::Ok()
                                        //                                          .body("Action wasn't a valid action.")
                                    }
                                };

                                {
                                    let mut shared_action_guard = shared_action_clone.lock().await;
                                    *shared_action_guard = SharedBLEAction::Command {
                                        device_uuid: uuid,
                                        action: action,
                                    };
                                }
                                Ok(())
                            }
                            .boxed()
                        })),
                        ..Default::default()
                    }),
                    notify: Some(CharacteristicNotify {
                        notify: false,
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let app_handle = adapter.serve_gatt_application(app).await.unwrap();

    println!("Service ready. Press enter to quit.");
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let _ = lines.next_line().await;

    println!("Removing service and advertisement");
    drop(app_handle);
    drop(adv_handle);
    sleep(Duration::from_secs(1)).await;
}

async fn await_for_inquiry_response(shared_action: Arc<Mutex<SharedBLEAction>>) -> usize {
    println!("Waiting???????????");
    loop {
        {
            let mut lock = shared_action.lock().await;
            match &*lock {
                SharedBLEAction::TargetResponse { ref target } => {
                    let thing = target.clone();
                    *lock = SharedBLEAction::NoUpdate;
                    return thing;
                }
                _ => {}
            }
        }
    }
}
