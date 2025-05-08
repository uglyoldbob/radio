#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum AndroidAutoMessageToPhone {
    /// A generic message to send to the phone
    Message(android_auto::SendableAndroidAutoMessage),
    Test,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum AndroidAutoMessageFromPhone {
    VideoContent(Vec<u8>),
}