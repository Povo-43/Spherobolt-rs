use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType, CharPropFlags};
use btleplug::platform::Manager;
use futures_util::stream::StreamExt;
use std::error::Error;
use tokio::time::{sleep, Duration};
use std::io::{self, Write};

#[derive(Debug, Default)]
struct SensorResponse {
    acc_x: f32,
    acc_y: f32,
    acc_z: f32,
}

fn parse_sensor_data(data: &[u8]) -> SensorResponse {
    let mut resp = SensorResponse::default();
    if data.len() >= 6 {
        resp.acc_x = i16::from_be_bytes([data[0], data[1]]) as f32 / 1000.0;
        resp.acc_y = i16::from_be_bytes([data[2], data[3]]) as f32 / 1000.0;
        resp.acc_z = i16::from_be_bytes([data[4], data[5]]) as f32 / 1000.0;
    }
    resp
}

async fn configure_sensor_stream(peripheral: &impl btleplug::api::Peripheral, characteristic_uuid: uuid::Uuid) -> Result<(), Box<dyn Error>> {
    let sensor_mask_command: [u8; 6] = [
        0x00, 0x01, 0x0F, 0x00, 0xFA, 0x00,
    ];

    let characteristics = peripheral.characteristics();
    let api_v2_char = characteristics.iter().find(|c| c.uuid == characteristic_uuid)
        .expect("API V2 Characteristic not found");

    peripheral.write(api_v2_char, &sensor_mask_command, WriteType::WithoutResponse).await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    let central = adapters.into_iter().nth(0).expect("No Bluetooth adapters");

    println!("Scanning for BLE devices...");
    central.start_scan(ScanFilter::default()).await?;
    sleep(Duration::from_secs(5)).await;

    let peripherals = central.peripherals().await?;
    let mut bolt_list = vec![];

    for (i, p) in peripherals.iter().enumerate() {
        let name = p.properties().await?.and_then(|props| props.local_name).unwrap_or("Unknown".to_string());
        println!("{}: {}", i, name);
        if name.contains("SB-") {
            bolt_list.push((i, p.clone()));
        }
    }

    if bolt_list.is_empty() {
        println!("No BOLT devices found.");
        return Ok(());
    }

    print!("Enter the number of the BOLT to connect: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let selected_index: usize = input.trim().parse()?;

    let (_, selected_peripheral) = bolt_list.into_iter()
        .find(|(idx, _)| *idx == selected_index)
        .expect("Invalid selection");

    println!("Connecting...");
    selected_peripheral.connect().await?;
    println!("Connected!");
    selected_peripheral.discover_services().await?;

    // UUIDはSphero V2 API V2 Characteristicのものに置き換える
    let api_v2_uuid = uuid::Uuid::parse_str("00010001-574f-4f20-5370-6865726f2121")?;

    configure_sensor_stream(&selected_peripheral, api_v2_uuid).await?;

    println!("Sensor stream configured. Listening for data...");

    let mut notifications = selected_peripheral.notifications().await?;
    while let Some(data) = notifications.next().await {
        if data.uuid == api_v2_uuid {
            let sensor = parse_sensor_data(&data.value);
            println!("Sensor Data: {:?}", sensor);
        }
    }

    Ok(())
}
