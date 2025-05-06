#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum AndroidAutoMessageToPhone {
    Test,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum AndroidAutoMessageFromPhone {
    VideoContent(Vec<u8>),
}