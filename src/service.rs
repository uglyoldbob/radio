#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]
#![warn(unused_extern_crates)]

//! This program is for handling the video and audio components for the radio

use std::sync::{Arc, Mutex};

use tokio::io::AsyncReadExt;
use video_service::VideoSource;

mod video_service;

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct MainConfiguration {
    /// The desired minimum debug level
    debug_level: Option<service::LogLevel>,
}

#[derive(Clone)]
/// The common data for an app user
pub struct AppUserCommon {
    video: Arc<Mutex<Vec<VideoSource>>>,
}

#[cfg(not(target_os = "android"))]
/// Processes a tcp connection from an app
pub async fn process_app(
    mut stream: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    common: AppUserCommon,
) -> Result<(), String> {
    use tokio::io::AsyncReadExt;
    println!("Processing an app at {:?}", addr);
    loop {
        let length = stream.read_u32().await.map_err(|e| e.to_string())?;
        let mut packet = vec![0; length as usize];
        stream.read_exact(&mut packet).await.map_err(|e| e.to_string())?;
        let packet: Result<(uobradio_comms::MessageFromApp, usize), bincode::error::DecodeError> =
            bincode::serde::decode_from_slice(&packet, bincode::config::standard());
        if let Ok((packet, _length)) = packet {
            match packet {
                uobradio_comms::MessageFromApp::GetCameraControls(id) => {
                    let packet = if let Ok(mut vid) = common.video.lock() {
                        if let Some(vid) = vid.get_mut(id as usize) {
                            let mut options = Vec::new();
                            for c in &vid.controls {
                                let sc = c.sendable();
                                let raw_sc = bincode::serde::encode_to_vec(sc, bincode::config::standard()).unwrap();
                                options.push(raw_sc);
                            }
                            Some(uobradio_comms::MessageToApp::CameraControls(id, options))
                        }
                        else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some(packet) = packet {
                        packet.send_to_stream(&mut stream).await?;
                    }
                }
                uobradio_comms::MessageFromApp::CameraSettingControl(id, data) => {
                    let a: uobradio_comms::v4l::control::Value = data.into();
                    let mut vid = common.video.lock().unwrap();
                    if let Some(vid) = vid.get_mut(id as usize) {

                    }
                }
                uobradio_comms::MessageFromApp::RequestCameraOptions => {
                    let packet = uobradio_comms::MessageToApp::CameraOptions(vec![0]);
                    packet.send_to_stream(&mut stream).await?;
                }
                uobradio_comms::MessageFromApp::Ping(id) => {
                    let packet = uobradio_comms::MessageToApp::PingReply(id);
                    packet.send_to_stream(&mut stream).await?;
                }
                uobradio_comms::MessageFromApp::RequestCamera(index) => {
                    let packet = if let Ok(video) = common.video.lock() {
                        if let Some(v) = video.get(index as usize) {
                            let frame = v.image.lock().unwrap();
                            let jpeg = frame.get_jpeg();
                            let response = uobradio_comms::MessageToApp::CameraDataJpeg(index, jpeg);
                            Some(response)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some(packet) = packet {
                        packet.send_to_stream(&mut stream).await?;
                    }
                }
                uobradio_comms::MessageFromApp::GpioControl(gpio) => match gpio {
                    uobradio_comms::Gpio::WinchControl(f, r) => println!("Winch control {} {}", f, r),
                    uobradio_comms::Gpio::CameraLedControl(i, s) => println!("Camera led {} to {}", i, s),
                    uobradio_comms::Gpio::LockDoors => println!("Received request to lock all doors"),
                    uobradio_comms::Gpio::UnlockDoors => println!("Recieved request to unlock all doors"),
                    uobradio_comms::Gpio::WindowControl { id, up, down } => {
                        println!("Window {} {}/{}", id, up, down)
                    }
                },
            }
        } else {
            println!("Failed to process packet");
            return Err("Received bad packet".to_string());
        }
    }
}

async fn udp_listener(_common: AppUserCommon) -> Result<(), String> {
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:13456").await.unwrap();
    println!("Starting radio listener");
    let mut response = vec![0; 1500];
    loop {
        while let Ok((n, addr)) = socket.recv_from(&mut response).await {
            let addr = addr.clone();
            println!("Got request from {:?} {} {:x?}", addr, n, &response[0..n]);
            let packet =
                bincode::serde::decode_from_slice(&response[0..n], bincode::config::standard());
            if let Ok((packet, _len)) = packet {
                match packet {
                    uobradio_comms::MessageFromApp::Ping(val) => {
                        println!("got ping packet {}", val);
                        let response = bincode::serde::encode_to_vec(
                            uobradio_comms::MessageToApp::PingReply(13457),
                            bincode::config::standard(),
                        )
                        .unwrap();
                        let _ = socket.send_to(&response, addr).await;
                    }
                    _ => {}
                }
            } else {
                println!("invalid packet received {:x?}", response);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn tcp_listener(common: AppUserCommon) -> Result<(), String> {
    let tcp = tokio::net::TcpListener::bind("0.0.0.0:13457").await;
    if let Ok(tcp) = tcp {
        loop {
            if let Ok((stream, addr)) = tcp.accept().await {
                let common2 = common.clone();
                let _ = tokio::task::spawn(async move {
                    let r = process_app(stream, addr, common2).await;
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

    let (_shutdown_send, mut shutdown_recv) = tokio::sync::mpsc::unbounded_channel::<()>();

    let mut vs = Vec::new();
    if let Ok(d) = uobradio_comms::v4l::Device::new(0) {
        vs.push(video_service::Video::video_start(d));
    }
    let common = AppUserCommon {
        video: Arc::new(Mutex::new(vs)),
    };

    let mut tasks: tokio::task::JoinSet<Result<(), String>> = tokio::task::JoinSet::new();
    let common2 = common.clone();
    tasks.spawn(async move { udp_listener(common2).await });
    let common2 = common.clone();
    tasks.spawn(async move { tcp_listener(common2).await });
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
