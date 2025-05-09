//! Android auto message types

/// An android auto message to send to the android auto device
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum AndroidAutoMessageToPhone {
    /// A generic message to send to the phone
    Message(android_auto::SendableAndroidAutoMessage),
    /// A dummy message type
    Test,
}

/// An android auto message received from the android auto device
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum AndroidAutoMessageFromPhone {
    /// H.264 video content
    VideoContent(Vec<u8>),
    /// The device disconected for an unknown reason
    Disconnect,
    /// The device connected
    Connect,
}