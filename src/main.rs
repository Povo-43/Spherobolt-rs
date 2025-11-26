use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, CharPropFlags, WriteType};
use btleplug::platform::Manager;
use std::error::Error;
use tokio::time::{sleep, Duration};
use std::io::{self, Write};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // BLE マネージャの作成
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    let central = adapters.into_iter().nth(0).expect("No Bluetooth adapters found");

    println!("Scanning for BLE devices...");
    central.start_scan(ScanFilter::default()).await?;
    sleep(Duration::from_secs(5)).await;

    let peripherals = central.peripherals().await?;
    let mut bolt_list = vec![];

    // デバイスに連番を振る
    println!("Found devices:");
    for (i, p) in peripherals.iter().enumerate() {
        let properties = p.properties().await?;
        let local_name = properties
            .as_ref()
            .and_then(|props| props.local_name.clone())
            .unwrap_or("Unknown".to_string());

        println!("{}: {}", i, local_name);

        if local_name.contains("SB-") {
            bolt_list.push((i, p.clone()));
        }
    }

    if bolt_list.is_empty() {
        println!("No BOLT devices found.");
        return Ok(());
    }

    // ユーザー入力で接続デバイスを選択
    print!("Enter the number of the BOLT to connect: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let selected_index: usize = input.trim().parse()?;

    // 選択したデバイスを接続
    let (_, peripheral) = bolt_list
        .into_iter()
        .find(|(idx, _)| *idx == selected_index)
        .expect("Invalid selection");

    let local_name = peripheral.properties().await?.unwrap().local_name.unwrap_or("Unknown".to_string());
    println!("Connecting to {}...", local_name);
    peripheral.connect().await?;
    println!("Connected!");

    // サービス探索
    peripheral.discover_services().await?;

    // Sphero V2 用 UUID
    let anti_dos_uuid = Uuid::parse_str("00020005574f4f2053706865726f2121")?;
    let api_v2_uuid = Uuid::parse_str("00010002574f4f2053706865726f2121")?;
    let dfu_uuid = Uuid::parse_str("00020002574f4f2053706865726f2121")?;

    // 特定の Characteristic を取得
    let chars = peripheral.characteristics();
    let anti_dos = chars.iter().find(|c| c.uuid == anti_dos_uuid).expect("antiDoSCharacteristic not found");
    let api_v2 = chars.iter().find(|c| c.uuid == api_v2_uuid).expect("apiV2Characteristic not found");
    let dfu = chars.iter().find(|c| c.uuid == dfu_uuid).expect("dfuControlCharacteristic not found");

// 認証文字列を書き込む
let auth_string = b"usetheforce...band";
peripheral.write(anti_dos, auth_string, WriteType::WithoutResponse).await?;
println!("antiDoS authentication sent.");

// 通知購読を有効化
if api_v2.properties.contains(CharPropFlags::NOTIFY) {
    peripheral.subscribe(api_v2).await?;
    println!("Subscribed to API V2 notifications.");
}
if dfu.properties.contains(CharPropFlags::NOTIFY) {
    peripheral.subscribe(dfu).await?;
    println!("Subscribed to DFU notifications.");
}


    println!("Handshake complete!");
    println!("Services:");
    for s in peripheral.services() {
        println!("  UUID: {}", s.uuid);
    }

    Ok(())
}
