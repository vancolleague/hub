//! Serves a Bluetooth GATT application using the callback programming model.
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use std::str::FromStr;

use bluer::{
    Uuid,
    adv::Advertisement,
    gatt::local::{
        Application, Characteristic, CharacteristicNotify, CharacteristicNotifyMethod, CharacteristicRead,
        CharacteristicWrite, CharacteristicWriteMethod, Service,
    },
};
use futures::FutureExt;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::Mutex,
    time::sleep,
};

// include!("gatt.inc");
const SERVICE_UUID: Uuid = Uuid::from_u128(0x36bc0fe1b00742809ec6b36c8bc98537);
const CHARACTERISTIC_UUID: Uuid = Uuid::from_u128(0x2a4fae8107134e1fa8187ac56e4f13e4);
const CHARACTERISTIC_UUID2: Uuid = Uuid::from_u128(0xa584507902e74f44b67902b90775abda);
#[allow(dead_code)]
const MANUFACTURER_ID: u16 = 0x45F1;

//#[tokio::main(flavor = "current_thread")]
pub async fn run_ble_server() { 
    println!("Starting ble server");
    dbg!(SERVICE_UUID);
    dbg!(CHARACTERISTIC_UUID);
    dbg!(CHARACTERISTIC_UUID2);
    let session = bluer::Session::new().await.unwrap();
    let adapter = session.default_adapter().await.unwrap();
    adapter.set_powered(true).await.unwrap();

    println!("Advertising on Bluetooth adapter {} with address {}", adapter.name(), adapter.address().await.unwrap());
    let mut manufacturer_data = BTreeMap::new();
    manufacturer_data.insert(MANUFACTURER_ID, vec![0x21, 0x22, 0x23, 0x24]);
    let le_advertisement = Advertisement {
        service_uuids: vec![SERVICE_UUID].into_iter().collect(),
        manufacturer_data,
        discoverable: Some(true),
        local_name: Some("gatt_server".to_string()),
        ..Default::default()
    };
    let adv_handle = adapter.advertise(le_advertisement).await.unwrap();

    println!("Serving GATT service on Bluetooth adapter {}", adapter.name());
    let value = Arc::new(Mutex::new(vec![0x10, 0x01, 0x01, 0x10]));
    let value_read = value.clone();
    let value_write = value.clone();
    let value_notify = value.clone();
    let value_read2 = value.clone();
    let value_write2 = value.clone();
    let value_notify2 = value.clone();
    let app = Application {
        services: vec![Service {
            uuid: SERVICE_UUID,
            primary: true,
            characteristics: vec![
            Characteristic {
                uuid: CHARACTERISTIC_UUID2,
                read: Some(CharacteristicRead{
                    read: true,
                    fun: Box::new(move |req| {
                        let value = value_read.clone();
                        async move {
                        let value = value.lock().await.clone();
                            println!("Read request {:?} with value {:x?}", &req, &value);
                            Ok(value)
                        }
                        .boxed()
                    }),
                    ..Default::default()
                }),
                write: Some(CharacteristicWrite {
                    write: true,
                    write_without_response: true,
                    method: CharacteristicWriteMethod::Fun(Box::new(move |new_value, req| {
                        let value = value_write.clone();
                        async move {
                            let text = std::str::from_utf8(&new_value).unwrap();
                            println!("Write request {:?} with value {:x?}", &req, &text);
                            // println!("Write request {:?} with value {:x?}", &req, &new_value);
                            let mut value = value.lock().await;
                            *value = new_value;
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
                                        if let Err(err) = notifier.notify(value.to_vec()).await {
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
            },
            Characteristic {
                uuid: CHARACTERISTIC_UUID,
                read: Some(CharacteristicRead {
                    read: true,
                    fun: Box::new(move |req| {
                        let value = value_read2.clone();
                        async move {
                            let value = value.lock().await.clone();
                            println!("Read request {:?} with value {:x?}", &req, &value);
                            Ok(value)
                        }
                        .boxed()
                    }),
                    ..Default::default()
                }),
                write: Some(CharacteristicWrite {
                    write: true,
                    write_without_response: true,
                    method: CharacteristicWriteMethod::Fun(Box::new(move |new_value, req| {
                        let value = value_write2.clone();
                        async move {
                            println!("Write request {:?} with value {:x?}", &req, &new_value);
                            let mut value = value.lock().await;
                            *value = new_value;
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
                        async move {
                            tokio::spawn(async move {
                                println!(
                                    "Notification session start with confirming={:?}",
                                    notifier.confirming()
                                );
                                let mut i = 0;
                                loop {
                                    {
                                        let mut value = value.lock().await;
                                        let text = format!("hello world {i}");
                                        println!("Notifying with value {:x?}", &*value);
                                        //if let Err(err) = notifier.notify(value.to_vec()).await {
                                        if let Err(err) = notifier.notify(text.as_bytes().to_vec()).await {
                                            println!("Notification error: {}", &err);
                                            break;
                                        }
                                        println!("Decrementing each element by one");
                                        for v in &mut *value {
                                            *v = v.saturating_sub(1);
                                        }
                                        i += 1;
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
        }],
        ..Default::default()
    };
//    let app_handle = adapter.serve_gatt_application(app).await?;

      adapter.serve_gatt_application(app).await.unwrap();
//    println!("Service ready. Press enter to quit.");
//    let stdin = BufReader::new(tokio::io::stdin());
//    let mut lines = stdin.lines();
//    let _ = lines.next_line().await;

//    println!("Removing service and advertisement");
//    drop(app_handle);
//    drop(adv_handle);
//    sleep(Duration::from_secs(1)).await;

//    Ok(())
}

