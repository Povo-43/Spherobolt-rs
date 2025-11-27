use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::Manager;
use futures_util::stream::StreamExt;
use std::error::Error;
use std::io::{self, Write};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

// センサーデータ構造体（必要なら拡張可）
#[derive(Debug, Default)]
struct SensorResponse {
    acc_x: f32,
    acc_y: f32,
    acc_z: f32,
}

// 加速度データのパース関数
fn parse_sensor_data(data: &[u8]) -> SensorResponse {
    let mut resp = SensorResponse::default();
    if data.len() >= 6 {
        resp.acc_x = i16::from_be_bytes([data[0], data[1]]) as f32 / 1000.0;
        resp.acc_y = i16::from_be_bytes([data[2], data[3]]) as f32 / 1000.0;
        resp.acc_z = i16::from_be_bytes([data[4], data[5]]) as f32 / 1000.0;
    }
    resp
}

// チェックサム計算関数
fn calculate_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &byte| acc.wrapping_add(byte))
}

// センサーマスクの設定
async fn configure_sensor_stream(peripheral: &impl btleplug::api::Peripheral, characteristic: &btleplug::api::Characteristic) -> Result<(), Box<dyn Error>> {
    let sensor_mask_command: [u8; 11] = [
        0x8D, 0x02, 0x11, 0x00, 0x01, 0x00, 0x0F, 0x00, 0xFA, 0x00, 0x1D
    ]; // ダミー値、必要に応じてフレーミング・チェックサムを計算する

    println!("[LOG] Sending sensor mask packet: {:?}", sensor_mask_command);
    peripheral.write(characteristic, &sensor_mask_command, btleplug::api::WriteType::WithoutResponse).await?;
    println!("[LOG] Sensor mask configured");
    Ok(())
}

// ping送信関数
async fn send_ping(peripheral: &impl btleplug::api::Peripheral, characteristic: &btleplug::api::Characteristic) -> Result<(), Box<dyn Error>> {
    let ping_payload: [u8; 8] = [
        0x8D, // Start of Packet
        0x0A, // Device ID: API Processor
        0x01, // Command ID: Ping
        0x00, 0x01, // Sequence
        0x00, // Flags
        calculate_checksum(&[0x0A, 0x01, 0x00, 0x01, 0x00]), // Checksum
        0xD8, // End of Packet
    ];
    println!("[LOG] Sending ping packet: {:?}", ping_payload);
    peripheral.write(characteristic, &ping_payload, btleplug::api::WriteType::WithoutResponse).await?;
    println!("[LOG] Ping sent!");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== BLE Scanner Start ===");
    let manager = Manager::new().await?;
    println!("[LOG] Manager acquired");

    let adapters = manager.adapters().await?;
    let central = adapters.into_iter().nth(0).expect("No Bluetooth adapters");
    println!("[LOG] Using adapter: {:?}", central.adapter_info().await?);

    println!("[LOG] Scan started");
    central.start_scan(ScanFilter::default()).await?;
    sleep(Duration::from_secs(5)).await;

    let peripherals = central.peripherals().await?;
    println!("[LOG] Found {} peripherals", peripherals.len());
    for (i, p) in peripherals.iter().enumerate() {
        let name = p.properties().await?.and_then(|props| props.local_name).unwrap_or("Unknown".to_string());
        println!("{}: {}", i, name);
    }

    print!("Enter the number of the device to connect: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let selected_index: usize = input.trim().parse()?;

    let peripheral = peripherals.get(selected_index).expect("Invalid selection");
    println!("[LOG] Selected device: {:?}", peripheral.properties().await?.and_then(|p| p.local_name));

    println!("[LOG] Connecting...");
    peripheral.connect().await?;
    println!("[LOG] Connected");

    peripheral.discover_services().await?;
    println!("[LOG] Services discovered");

    println!("=== Services ===");
    for service in peripheral.services() {
        println!("SERVICE: {}", service.uuid);
        for c in &service.characteristics {
            println!("  CHAR: {}  props: {:?}", c.uuid, c.properties);
        }
    }

    // API V2 Characteristic UUID
    let api_v2_uuid = Uuid::parse_str("00010002-574f-4f20-5370-6865726f2121")?;
    let characteristics = peripheral.characteristics(); // 一時値ではなく束縛
    let char_opt = characteristics.iter().find(|c| c.uuid == api_v2_uuid);
    let api_char = match char_opt {
        Some(c) => c,
        None => {
            eprintln!("[ERROR] API V2 Characteristic not found!");
            return Ok(());
        }
    };
    println!("[LOG] API V2 Characteristic found: {:?}", api_char);

    peripheral.subscribe(api_char).await?;
    println!("[LOG] Notifications enabled");

    send_ping(peripheral, api_char).await?;
    configure_sensor_stream(peripheral, api_char).await?;

    println!("=== Listening for notifications ===");
    let mut notifications = peripheral.notifications().await?;
    while let Some(data) = notifications.next().await {
        if data.uuid == api_v2_uuid {
            println!("[NOTIF] Data: {:?}", data.value);
            let sensor = parse_sensor_data(&data.value);
            println!("[SENSOR] {:?}", sensor);
        }
    }

    Ok(())
}
