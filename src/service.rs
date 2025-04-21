#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]
#![warn(unused_extern_crates)]

//! This program is for handling the video and audio components for the radio

use tokio::io::AsyncReadExt;

#[path = "../android2/src/comms.rs"]
mod comms;

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct MainConfiguration {
    /// The desired minimum debug level
    debug_level: Option<service::LogLevel>,
}

#[cfg(not(target_os = "android"))]
/// Processes a tcp connection from an app
pub async fn process_app(
    mut stream: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
) -> Result<(), ()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    println!("Processing an app at {:?}", addr);
    loop {
        let length = stream.read_u32().await.map_err(|_| ())?;
        let mut packet = vec![0; length as usize];
        stream.read_exact(&mut packet).await.map_err(|_| ())?;
        let packet: Result<(comms::MessageFromApp, usize), bincode::error::DecodeError> =
            bincode::serde::decode_from_slice(&packet, bincode::config::standard());
        if let Ok((packet, _length)) = packet {
            match packet {
                comms::MessageFromApp::Ping(id) => {
                    println!("Received ping packet from user: {}", id);
                }
                comms::MessageFromApp::RequestCamera(_index) => {
                    println!("Processing request for camera image");
                    let rval: Option<comms::MessageToApp> = None;
                    if let Some(response) = rval {
                        println!("Got message to app");
                        let packet =
                            bincode::serde::encode_to_vec(response, bincode::config::standard())
                                .unwrap();
                        let _ = stream
                            .write_all(&((packet.len() as u32).to_be_bytes()[0..4]))
                            .await;
                        let _ = stream.write_all(&packet).await;
                        println!("Done sending message to app");
                    }
                }
                comms::MessageFromApp::GpioControl(gpio) => {
                    match gpio {
                        comms::Gpio::WinchControl(f, r) => println!("Winch control {} {}", f, r),
                        comms::Gpio::CameraLedControl(i, s) => println!("Camera led {} to {}", i, s),
                        comms::Gpio::LockDoors => println!("Received request to lock all doors"),
                        comms::Gpio::UnlockDoors => println!("Recieved request to unlock all doors"),
                        comms::Gpio::WindowControl { id, up, down } => println!("Window {} {}/{}", id, up, down),
                    }
                }
            }
        }
    }
}

#[cfg(not(target_os = "android"))]
async fn tcp_listener() -> Result<(), String> {
    let tcp = tokio::net::TcpListener::bind("0.0.0.0:13457").await;
    if let Ok(tcp) = tcp {
        loop {
            if let Ok((stream, addr)) = tcp.accept().await {
                let _ = tokio::task::spawn(async move {
                    let r = process_app(stream, addr).await;
                    println!("Completed handling user {:?}", r);
                    r
                })
                .await
                .unwrap();
            }
        }
    } else {
        panic!("Unable to open tcp listener to listen for apps connecting");
    }
}

/// The main function for the service
async fn smain() {
    #[cfg(target_family = "windows")]
    {
        std::panic::set_hook(Box::new(|p| {
            service::log::debug!("Panic {:?}", p);
        }))
    }

    let f = tokio::fs::File::open("./service.toml").await;
    let settings = if let Ok(mut f) = f {
        let mut config_raw = Vec::new();
        f.read_to_end(&mut config_raw).await.unwrap();
        let config_str = String::from_utf8(config_raw).unwrap();
        toml::from_str(&config_str).unwrap()
    } else {
        MainConfiguration::default()
    };

    service::log::set_max_level(
        settings
            .debug_level
            .as_ref()
            .unwrap_or(&service::LogLevel::Trace)
            .level_filter(),
    );

    let (shutdown_send, mut shutdown_recv) = tokio::sync::mpsc::unbounded_channel::<()>();

    let mut tasks: tokio::task::JoinSet<Result<(), String>> = tokio::task::JoinSet::new();
    tasks.spawn(async { comms::udp_listener().await });
    tasks.spawn(async { tcp_listener().await });
    tokio::select! {
        r = tasks.join_next() => {
            service::log::error!("A task exited {:?}, closing server in 5 seconds", r);
            tokio::time::sleep(tokio::time::Duration::from_millis(5000)).await;
        }
        _ = tokio::signal::ctrl_c() => {}
        _ = shutdown_recv.recv() => {}
    }
    service::log::error!("Closing server now");
}

service::ServiceAsyncMacro!(service_starter, smain, u64);

#[tokio::main]
async fn main() -> Result<(), u32> {
    let service = service::Service::new("uobradio".to_string());
    service.new_log(service::LogLevel::Debug);
    if let Err(e) = service::DispatchAsync!(service, service_starter) {
        Err(e)
    } else {
        Ok(())
    }
}
