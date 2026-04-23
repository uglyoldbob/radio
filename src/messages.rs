//! Code for bluetooth messages
//! A linux example for the bluetooth-rust crate

use std::io::{Read, Write};

use bluetooth_rust::{
    BluetoothAdapterBuilder, BluetoothAdapterTrait, BluetoothDevice, BluetoothDeviceTrait,
    BluetoothSocket, BluetoothSocketTrait,
};

pub enum MessageType {
    Email,
    SmsGsm,
    SmsCdma,
    Mms,
}

impl std::fmt::Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            MessageType::Email => "EMAIL",
            MessageType::SmsGsm => "SMS_GSM",
            MessageType::SmsCdma => "SMS_CDMA",
            MessageType::Mms => "MMS",
        };
        f.write_str(s)?;
        Ok(())
    }
}

impl MessageType {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "EMAIL" => Ok(Self::Email),
            "SMS_GSM" => Ok(Self::SmsGsm),
            "SMS_CDMA" => Ok(Self::SmsCdma),
            "MMS" => Ok(Self::Mms),
            _ => Err(format!("Unknown type {s}")),
        }
    }
}

#[derive(Default)]
pub struct VCard {
    version: String,
    formatted_name: Option<String>,
    name: Option<String>,
    numbers: Vec<String>,
    emails: Vec<String>,
    bt_uid: Vec<String>,
    bt_uci: Vec<String>,
}

impl std::fmt::Display for VCard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BEGIN:VCARD\r\n")?;
        f.write_str(&format!("VERSION:{}\r\n", self.version))?;
        if self.version.as_str() == "3.0" {
            if let Some(formatted_name) = &self.formatted_name {
                f.write_str(&format!("FN:{}\r\n", formatted_name))?;
            }
        }
        if let Some(name) = &self.name {
            f.write_str(&format!("N:{}\r\n", name))?;
        }
        for n in &self.numbers {
            f.write_str(&format!("TEL:{}\r\n", n))?;
        }
        for n in &self.emails {
            f.write_str(&format!("EMAIL:{}\r\n", n))?;
        }
        for n in &self.bt_uid {
            f.write_str(&format!("X-BT-UID:{}\r\n", n))?;
        }
        for n in &self.bt_uci {
            f.write_str(&format!("X-BT-UCI:{}\r\n", n))?;
        }
        f.write_str("END:VCARD\r\n")?;
        Ok(())
    }
}

impl VCard {
    pub fn parse(c: &mut std::io::Lines<std::io::Cursor<&str>>) -> Result<Self, String> {
        let mut out = Self::default();
        if let Some(Ok(line)) = c.next() {
            if line.as_str() != "BEGIN:VCARD" {
                return Err("No begin line found".to_string());
            }
        } else {
            return Err("No begin line found".to_string());
        }
        loop {
            if let Some(Ok(line)) = c.next() {
                if line.as_str() == "END:VCARD" {
                    break;
                }
                if line.starts_with("VERSION:") {
                    if let Some(v) = line.split_once(":") {
                        out.version = v.1.to_string();
                    }
                }
                if line.starts_with("FN:") {
                    if let Some(v) = line.split_once(":") {
                        out.formatted_name = Some(v.1.to_string());
                    }
                }
                if line.starts_with("N:") {
                    if let Some(v) = line.split_once(":") {
                        out.name = Some(v.1.to_string());
                    }
                }
                if line.starts_with("TEL:") {
                    if let Some(v) = line.split_once(":") {
                        out.numbers.push(v.1.to_string());
                    }
                }
                if line.starts_with("EMAIL:") {
                    if let Some(v) = line.split_once(":") {
                        out.emails.push(v.1.to_string());
                    }
                }
                if line.starts_with("X-BT-UID:") {
                    if let Some(v) = line.split_once(":") {
                        out.bt_uid.push(v.1.to_string());
                    }
                }
                if line.starts_with("X-BT-UCI:") {
                    if let Some(v) = line.split_once(":") {
                        out.bt_uci.push(v.1.to_string());
                    }
                }
            } else {
                return Err("Not enough lines found".to_string());
            }
        }
        Ok(out)
    }
}

pub struct BMessage {
    status_read: bool,
    mtype: MessageType,
    folder: String,
    originator: Vec<VCard>,
}

fn last_512(s: &str) -> String {
    let len = s.chars().count();

    s.chars().skip(len.saturating_sub(512)).collect()
}

impl std::fmt::Display for BMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BEGIN:BMSG\r\n")?;
        f.write_str("VERSION:1.0\r\n")?;
        f.write_str(&format!(
            "STATUS:{}\r\n",
            if self.status_read { "READ" } else { "UNREAD" }
        ))?;
        f.write_str(&format!("TYPE:{}\r\n", self.mtype))?;
        f.write_str(&format!("FOLDER:{}\r\n", &last_512(&self.folder)))?;
        for o in &self.originator {
            f.write_str(&format!("{}", o))?;
        }
        f.write_str("END:BMSG\r\n")?;
        Ok(())
    }
}

#[derive(Default)]
struct ObexConnect {
    version: u8,
    flags: u8,
    max_packet: u16,
    headers: Vec<Vec<u8>>,
}

impl ObexConnect {
    fn new(max_packet: u16) -> Self {
        Self {
            version: 0x10,
            flags: 0x00,
            max_packet,
            headers: vec![],
        }
    }

    /// 0x46 Target header (byte sequence)
    fn target(mut self, data: &[u8]) -> Self {
        let mut h = Vec::with_capacity(3 + data.len());

        h.push(0x46); // Target header
        h.extend_from_slice(&(data.len() as u16 + 3).to_be_bytes());
        h.extend_from_slice(data);

        self.headers.push(h);
        self
    }

    /// Add raw byte-sequence header (0x42 style)
    fn byte_seq(mut self, header_id: u8, data: &[u8]) -> Self {
        let mut h = Vec::with_capacity(3 + data.len());

        h.push(header_id);
        h.extend_from_slice(&(data.len() as u16 + 3).to_be_bytes());
        h.extend_from_slice(data);

        self.headers.push(h);
        self
    }

    fn build(self) -> Vec<u8> {
        let mut packet = Vec::new();

        // placeholder for opcode + length
        packet.push(0x80); // CONNECT
        packet.extend_from_slice(&[0x00, 0x00]); // filled later

        // fixed fields
        packet.push(self.version);
        packet.push(self.flags);
        packet.extend_from_slice(&self.max_packet.to_be_bytes());

        // headers
        for h in &self.headers {
            packet.extend_from_slice(h);
        }

        // fix length
        let len = packet.len() as u16;
        packet[1..3].copy_from_slice(&len.to_be_bytes());

        packet
    }
}

#[derive(Debug)]
pub struct ObexConnectResponse {
    pub response_code: u8,
    pub packet_length: u16,
    pub obex_version: u8,
    pub flags: u8,
    pub max_packet_length: u16,
    pub headers: Vec<ObexHeader>,
}

#[derive(Debug)]
pub enum ObexHeader {
    ConnectionId(u32),
    Who(Vec<u8>),
    Target(Vec<u8>),
    Unknown { id: u8, data: Vec<u8> },
}

#[derive(Debug)]
pub enum ObexParseError {
    UnexpectedEof,
    InvalidResponseCode(u8),
    InvalidPacketLength,
}

impl std::fmt::Display for ObexParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "Unexpected end of data"),
            Self::InvalidResponseCode(c) => write!(f, "Invalid response code: {:#X}", c),
            Self::InvalidPacketLength => write!(f, "Packet length mismatch"),
        }
    }
}

impl std::error::Error for ObexParseError {}

impl ObexConnectResponse {
    pub fn parse(data: &[u8]) -> Result<Self, ObexParseError> {
        if data.len() < 7 {
            return Err(ObexParseError::UnexpectedEof);
        }

        let response_code = data[0];

        // Final bit (0x80) must be set; success codes are 0xA0 (200 OK)
        if response_code & 0x80 == 0 {
            return Err(ObexParseError::InvalidResponseCode(response_code));
        }

        let packet_length = u16::from_be_bytes([data[1], data[2]]);

        if data.len() < packet_length as usize {
            return Err(ObexParseError::InvalidPacketLength);
        }

        let obex_version = data[3];
        let flags = data[4];
        let max_packet_length = u16::from_be_bytes([data[5], data[6]]);

        // Parse optional headers starting at offset 7
        let headers = parse_headers(&data[7..packet_length as usize])?;

        Ok(Self {
            response_code,
            packet_length,
            obex_version,
            flags,
            max_packet_length,
            headers,
        })
    }

    pub fn is_success(&self) -> bool {
        (self.response_code & 0x7F) == 0x20 // 0xA0 with final bit masked
    }

    pub fn connection_id(&self) -> Option<u32> {
        self.headers.iter().find_map(|h| {
            if let ObexHeader::ConnectionId(id) = h {
                Some(*id)
            } else {
                None
            }
        })
    }

    pub fn who_uuid(&self) -> Option<&[u8]> {
        self.headers.iter().find_map(|h| {
            if let ObexHeader::Who(data) = h {
                Some(data.as_slice())
            } else {
                None
            }
        })
    }
}

fn parse_headers(mut data: &[u8]) -> Result<Vec<ObexHeader>, ObexParseError> {
    let mut headers = Vec::new();

    while !data.is_empty() {
        let id = data[0];
        let encoding = id & 0xC0; // top 2 bits define the header encoding

        let header = match encoding {
            // 0x00 = Unicode string (null-terminated, 2-byte length)
            // 0x40 = Byte sequence (2-byte length)
            0x00 | 0x40 => {
                if data.len() < 3 {
                    return Err(ObexParseError::UnexpectedEof);
                }
                let length = u16::from_be_bytes([data[1], data[2]]) as usize;
                if data.len() < length {
                    return Err(ObexParseError::UnexpectedEof);
                }
                let body = data[3..length].to_vec();
                data = &data[length..];

                match id {
                    0x4A => ObexHeader::Who(body),
                    0x46 => ObexHeader::Target(body),
                    _ => ObexHeader::Unknown { id, data: body },
                }
            }

            // 0x80 = 1-byte value (no length field)
            0x80 => {
                if data.len() < 2 {
                    return Err(ObexParseError::UnexpectedEof);
                }
                let byte = data[1];
                data = &data[2..];
                ObexHeader::Unknown {
                    id,
                    data: vec![byte],
                }
            }

            // 0xC0 = 4-byte value (no length field)
            0xC0 => {
                if data.len() < 5 {
                    return Err(ObexParseError::UnexpectedEof);
                }
                let value = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
                data = &data[5..];

                match id {
                    0xCB => ObexHeader::ConnectionId(value),
                    _ => ObexHeader::Unknown {
                        id,
                        data: value.to_be_bytes().to_vec(),
                    },
                }
            }

            _ => return Err(ObexParseError::UnexpectedEof),
        };

        headers.push(header);
    }

    Ok(headers)
}

#[derive(Debug)]
pub enum SetpathDirection {
    Root,          // flags = 0x03, no Name header
    Parent,        // flags = 0x02, no Name header
    Child(String), // flags = 0x00, Name header with folder name
}

impl SetpathDirection {
    pub fn build(&self, connection_id: Option<u32>) -> Vec<u8> {
        let flags: u8 = match self {
            SetpathDirection::Root => 0x03,
            SetpathDirection::Parent => 0x02,
            SetpathDirection::Child(_) => 0x00,
        };

        let mut pkt: Vec<u8> = Vec::new();

        // Opcode + placeholder length + flags + constants
        pkt.push(0x85);
        pkt.extend_from_slice(&[0x00, 0x00]); // length placeholder
        pkt.push(flags);
        pkt.push(0x00); // constants

        // Optional: Connection-ID header
        if let Some(id) = connection_id {
            pkt.push(0xCB);
            pkt.extend_from_slice(&id.to_be_bytes());
        }

        // Optional: Name header (only for Child)
        if let SetpathDirection::Child(name) = self {
            // Encode as UTF-16BE + null terminator
            let mut utf16: Vec<u8> = name.encode_utf16().flat_map(|c| c.to_be_bytes()).collect();
            utf16.extend_from_slice(&[0x00, 0x00]); // null terminator

            let header_len = (3 + utf16.len()) as u16;
            pkt.push(0x01); // Name header ID
            pkt.extend_from_slice(&header_len.to_be_bytes());
            pkt.extend_from_slice(&utf16);
        }

        // Patch in total length
        let total = pkt.len() as u16;
        pkt[1] = (total >> 8) as u8;
        pkt[2] = (total & 0xFF) as u8;

        pkt
    }
}

#[derive(Debug)]
pub struct MapGetMessagesListing {
    pub connection_id: u32,
    pub max_list_count: u16,
    pub list_start_offset: u16,
    pub filter_message_type: u8,
    pub filter_read_status: u8,
    pub subject_length: Option<u8>,
    pub filter_period_begin: Option<String>,
    pub filter_period_end: Option<String>,
}

impl MapGetMessagesListing {
    pub fn serialize(&self) -> Vec<u8> {
        let type_str = b"x-bt/MAP-msg-listing\0";

        let mut app_params: Vec<u8> = Vec::new();
        app_params.extend_from_slice(&[
            0x01,
            0x02,
            (self.max_list_count >> 8) as u8,
            (self.max_list_count & 0xFF) as u8,
        ]);
        app_params.extend_from_slice(&[
            0x02,
            0x02,
            (self.list_start_offset >> 8) as u8,
            (self.list_start_offset & 0xFF) as u8,
        ]);
        app_params.extend_from_slice(&[0x03, 0x01, self.filter_message_type]);
        app_params.extend_from_slice(&[0x06, 0x01, self.filter_read_status]);

        let mut pkt = vec![0x83, 0x00, 0x00]; // GET | Final

        pkt.push(0xCB);
        pkt.extend_from_slice(&self.connection_id.to_be_bytes());

        // NO Name header — Android uses mCurrentFolder (set by SETPATH)

        let type_len = (3 + type_str.len()) as u16;
        pkt.push(0x42);
        pkt.extend_from_slice(&type_len.to_be_bytes());
        pkt.extend_from_slice(type_str);

        let app_len = (3 + app_params.len()) as u16;
        pkt.push(0x4C);
        pkt.extend_from_slice(&app_len.to_be_bytes());
        pkt.extend_from_slice(&app_params);

        let total_len = pkt.len() as u16;
        pkt[1] = (total_len >> 8) as u8;
        pkt[2] = (total_len & 0xFF) as u8;
        pkt
    }
}

#[derive(Debug, Clone)]
pub struct MapMessage {
    pub handle: String,
    pub subject: Option<String>,
    pub datetime: Option<String>,
    pub sender_name: Option<String>,
    pub sender_addressing: Option<String>,
    pub recipient_addressing: Option<String>,
    pub msg_type: Option<MapMessageType>,
    pub size: Option<u32>,
    pub read: bool,
    pub sent: bool,
    pub priority: bool,
    pub protected: bool,
    pub reception_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MapMessageType {
    SmsGsm,
    SmsCdma,
    Mms,
    Email,
    Unknown(String),
}

impl MapMessageType {
    fn from_str(s: &str) -> Self {
        match s {
            "SMS_GSM" => Self::SmsGsm,
            "SMS_CDMA" => Self::SmsCdma,
            "MMS" => Self::Mms,
            "EMAIL" => Self::Email,
            other => Self::Unknown(other.to_string()),
        }
    }
}

#[derive(Debug)]
pub struct MapListingResponse {
    pub response_code: u8,
    pub connection_id: Option<u32>,
    pub app_params: MapListingAppParams,
    pub xml_body: String,
    pub messages: Vec<MapMessage>,
}

#[derive(Debug, Default)]
pub struct MapListingAppParams {
    pub new_message: Option<bool>, // tag 0x0D
    pub mse_time: Option<String>,  // tag 0x02 — device timestamp
    pub listing_size: Option<u16>, // tag 0x11
}

impl MapListingResponse {
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() < 3 {
            return Err("Too short".into());
        }

        let response_code = data[0];
        let packet_length = u16::from_be_bytes([data[1], data[2]]) as usize;
        let data = &data[..packet_length.min(data.len())];

        let mut pos = 3;
        let mut connection_id = None;
        let mut app_params = MapListingAppParams::default();
        let mut xml_body = Vec::new();

        while pos < data.len() {
            let header_id = data[pos];
            pos += 1;

            match header_id {
                // Connection-ID — 4-byte value
                0xCB => {
                    if pos + 4 > data.len() {
                        break;
                    }
                    connection_id = Some(u32::from_be_bytes([
                        data[pos],
                        data[pos + 1],
                        data[pos + 2],
                        data[pos + 3],
                    ]));
                    pos += 4;
                }

                // Application Parameters — byte sequence
                0x4C => {
                    if pos + 2 > data.len() {
                        break;
                    }
                    let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                    pos += 2;
                    let end = pos + len - 3;
                    app_params = parse_map_listing_app_params(&data[pos..end.min(data.len())]);
                    pos = end;
                }

                // Body (0x48) or End-of-Body (0x49)
                0x48 | 0x49 => {
                    if pos + 2 > data.len() {
                        break;
                    }
                    let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                    pos += 2;
                    let data_len = len - 3;
                    if pos + data_len > data.len() {
                        break;
                    }
                    xml_body.extend_from_slice(&data[pos..pos + data_len]);
                    pos += data_len;
                }

                // Skip unknown byte-sequence headers (top 2 bits = 01)
                id if (id & 0xC0) == 0x40 => {
                    if pos + 2 > data.len() {
                        break;
                    }
                    let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                    pos += (len - 1).max(2);
                }

                // Skip unknown 4-byte headers
                id if (id & 0xC0) == 0xC0 => {
                    pos += 4;
                }

                // Skip unknown 1-byte headers
                id if (id & 0xC0) == 0x80 => {
                    pos += 1;
                }

                _ => break,
            }
        }

        let xml_str =
            String::from_utf8(xml_body).map_err(|e| format!("Invalid UTF-8 in XML: {}", e))?;

        let messages = parse_msg_listing_xml(&xml_str);

        Ok(Self {
            response_code,
            connection_id,
            app_params,
            xml_body: xml_str,
            messages,
        })
    }

    pub fn is_success(&self) -> bool {
        (self.response_code & 0x7F) == 0x20
    }
}

fn parse_map_listing_app_params(data: &[u8]) -> MapListingAppParams {
    let mut params = MapListingAppParams::default();
    let mut pos = 0;

    while pos + 2 <= data.len() {
        let tag = data[pos];
        let len = data[pos + 1] as usize;
        pos += 2;

        if pos + len > data.len() {
            break;
        }
        let value = &data[pos..pos + len];
        pos += len;

        match tag {
            0x0D => {
                // NewMessage
                params.new_message = Some(value.first().copied().unwrap_or(0) != 0);
            }
            0x12 => {
                // ListingSize — it's 0x12, not 0x11!
                if value.len() >= 2 {
                    params.listing_size = Some(u16::from_be_bytes([value[0], value[1]]));
                }
            }
            0x19 => {
                // MseTime — it's 0x19, not 0x02!
                params.mse_time = String::from_utf8(value.to_vec()).ok();
            }
            _ => {}
        }
    }

    params
}

fn parse_msg_listing_xml(xml: &str) -> Vec<MapMessage> {
    let mut messages = Vec::new();
    let mut search = xml;

    while let Some(start) = search.find("<msg ") {
        search = &search[start..];
        match search.find("/>") {
            Some(end) => {
                let tag = &search[..end + 2];
                if let Ok(msg) = parse_msg_tag(tag) {
                    messages.push(msg);
                }
                search = &search[end + 2..];
            }
            None => break,
        }
    }

    messages
}

fn parse_msg_tag(tag: &str) -> Result<MapMessage, String> {
    let get = |attr: &str| -> Option<String> {
        let needle = format!("{}=\"", attr);
        let start = tag.find(&needle)? + needle.len();
        let end = tag[start..].find('"')? + start;
        Some(tag[start..end].to_string())
    };

    Ok(MapMessage {
        handle: get("handle").ok_or("missing handle")?,
        subject: get("subject"),
        datetime: get("datetime"),
        sender_name: get("sender_name"),
        sender_addressing: get("sender_addressing"),
        recipient_addressing: get("recipient_addressing"),
        msg_type: get("type").map(|t| MapMessageType::from_str(&t)),
        size: get("size").and_then(|s| s.parse().ok()),
        read: get("read")
            .map(|v| v.eq_ignore_ascii_case("yes"))
            .unwrap_or(false),
        sent: get("sent")
            .map(|v| v.eq_ignore_ascii_case("yes"))
            .unwrap_or(false),
        priority: get("priority")
            .map(|v| v.eq_ignore_ascii_case("yes"))
            .unwrap_or(false),
        protected: get("protected")
            .map(|v| v.eq_ignore_ascii_case("yes"))
            .unwrap_or(false),
        reception_status: get("reception_status"),
    })
}

pub fn build_get_folder_listing(connection_id: u32) -> Vec<u8> {
    let type_str = b"x-obex/folder-listing\0"; // ← was x-bt/folderListing

    let mut pkt = vec![0x83, 0x00, 0x00];

    pkt.push(0xCB);
    pkt.extend_from_slice(&connection_id.to_be_bytes());

    let type_len = (3 + type_str.len()) as u16;
    pkt.push(0x42);
    pkt.extend_from_slice(&type_len.to_be_bytes());
    pkt.extend_from_slice(type_str);

    let total = pkt.len() as u16;
    pkt[1] = (total >> 8) as u8;
    pkt[2] = (total & 0xFF) as u8;
    pkt
}

pub fn extract_body(data: &[u8]) -> String {
    if data.len() < 3 {
        return String::new();
    }

    let packet_length = u16::from_be_bytes([data[1], data[2]]) as usize;
    let data = &data[..packet_length.min(data.len())];

    let mut pos = 3;
    let mut body = Vec::new();

    while pos < data.len() {
        if pos >= data.len() {
            break;
        }
        let header_id = data[pos];
        pos += 1;

        match header_id {
            // Body (0x48) or End-of-Body (0x49)
            0x48 | 0x49 => {
                if pos + 2 > data.len() {
                    break;
                }
                let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                pos += 2;
                let data_len = len.saturating_sub(3);
                if pos + data_len > data.len() {
                    break;
                }
                body.extend_from_slice(&data[pos..pos + data_len]);
                pos += data_len;
            }

            // 4-byte value headers (Connection-ID etc.)
            id if (id & 0xC0) == 0xC0 => {
                if pos + 4 > data.len() {
                    break;
                }
                pos += 4;
            }

            // Byte-sequence headers (2-byte length prefix)
            id if (id & 0xC0) == 0x40 || (id & 0xC0) == 0x00 => {
                if pos + 2 > data.len() {
                    break;
                }
                let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                if len < 3 {
                    break;
                }
                pos += len - 1; // -1 because we already consumed the header_id byte
            }

            // 1-byte value headers
            id if (id & 0xC0) == 0x80 => {
                if pos + 1 > data.len() {
                    break;
                }
                pos += 1;
            }

            _ => break,
        }
    }

    String::from_utf8(body.clone()).unwrap_or_else(|_| {
        // If not valid UTF-8, show hex for debugging
        body.iter().map(|b| format!("{:02X} ", b)).collect()
    })
}

struct MessageClient {
    socket: BluetoothSocket,
    message_handle: u32,
    session_ok: bool,
}

impl MessageClient {
    pub fn new(mut socket: BluetoothSocket) -> Self {
        log::info!("Socket is connected? {:?}", socket.is_connected());
        // Use a tokio-aware sleep so the async runtime (including the MNS
        // acceptor task) keeps making progress while we wait for the RFCOMM
        // connection to settle before sending the OBEX CONNECT.
        // Because we now process each device immediately after connecting
        // there is no multi-second idle window during which the phone could
        // queue unsolicited bytes, so no explicit drain is needed.
        // Wait for the RFCOMM connection to fully settle, including Bluetooth
        // security negotiation (authentication / encryption).  The kernel
        // completes the L2CAP + RFCOMM handshake before connect() returns, but
        // the security layer runs asynchronously on some stacks and can take
        // several hundred ms.  Writing too soon produces ENOTCONN.
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(tokio::time::sleep(std::time::Duration::from_secs(1)))
        });
        log::info!("Settle wait done, sending OBEX CONNECT");
        // this is the value for mas obex target
        // MAP MAS OBEX target UUID: BB582B40-420C-11DB-B0DE-0800200C9A66
        let mas_obex_uuid = [
            0xBB, 0x58, 0x2B, 0x40, 0x42, 0x0C, 0x11, 0xDB, 0xB0, 0xDE, 0x08, 0x00, 0x20, 0x0C,
            0x9A, 0x66,
        ];

        // MAP 1.2+ requires MapSupportedFeatures (tag 0x27, 4 bytes) in the
        // OBEX CONNECT Application Parameters. Without it many phones reply
        // 0xC6 (Not Acceptable) and refuse subsequent operations.
        // Bits 0-4: Notification Registration, Notification, Browsing,
        //           Uploading, Delete features (= MAP 1.2 baseline).
        let map_supported_features: [u8; 6] = [
            0x27, 0x04, // tag=MapSupportedFeatures, length=4
            0x00, 0x00, 0x00, 0x1F, // features bitmap
        ];
        let packet = ObexConnect::new(0x8000)
            .target(&mas_obex_uuid)
            .byte_seq(0x4C, &map_supported_features) // APPLICATION PARAMETERS
            .build();
        let a = socket.write_all(&packet);
        let b = socket.flush();
        log::info!("Sent OBEX CONNECT: {:?} {:?} {:x?}", a, b, packet);

        // If the write itself failed the socket is dead — skip the read so we
        // don't block for several seconds waiting for data that will never come.
        if a.is_err() {
            log::error!(
                "OBEX CONNECT write failed ({:?}) — socket is dead, giving up on this device",
                a
            );
            return Self {
                socket,
                message_handle: 0,
                session_ok: false,
            };
        }

        let mut buf = [0u8; 1024];
        let mut message_handle = 0;
        let mut session_ok = false;
        if let Ok(a) = socket.read(&mut buf) {
            if a > 0 {
                log::info!("OBEX CONNECT response ({} bytes): {:x?}", a, &buf[0..a]);
                let resp = ObexConnectResponse::parse(&buf[0..a]);
                log::info!("Parsed response: {:x?}", resp);
                if let Ok(r) = resp {
                    if r.is_success() {
                        log::info!("OBEX CONNECT accepted (0xA0 OK)");
                        session_ok = true;
                    } else {
                        log::warn!(
                            "OBEX CONNECT non-success code {:#04X} — proceeding if ConnectionId present",
                            r.response_code
                        );
                    }
                    if let Some(i) = r.connection_id() {
                        log::info!("Got ConnectionId = {}", i);
                        message_handle = i;
                        // Treat any response that provides a ConnectionId as a
                        // usable session — some phones reply with a non-0xA0 code
                        // (e.g. 0xC6) but still assign a valid connection ID.
                        session_ok = true;
                    } else {
                        log::error!(
                            "No ConnectionId in OBEX CONNECT response — session not usable"
                        );
                    }
                }
            }
        } else {
            log::error!("Failed to read OBEX CONNECT response — socket is dead");
        }
        // Give the phone's MAP session a moment to finish initialising after
        // the OBEX CONNECT before we send any further operations.  Some phones
        // (especially ones that do internal setup asynchronously) return 0xC0
        // Bad Request if we send the notification registration too quickly.
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(tokio::time::sleep(std::time::Duration::from_millis(500)))
        });
        Self {
            socket,
            message_handle,
            session_ok,
        }
    }

    /// Returns true if the OBEX session was successfully established.
    pub fn has_session(&self) -> bool {
        self.session_ok
    }

    /// Read exactly one complete OBEX packet
    fn read_obex_packet(&mut self) -> Option<Vec<u8>> {
        // Read the 3-byte fixed header first
        let mut header = [0u8; 3];
        self.socket.read_exact(&mut header).ok()?;

        let packet_len = u16::from_be_bytes([header[1], header[2]]) as usize;

        if packet_len < 3 {
            log::error!("Invalid packet length: {}", packet_len);
            return None;
        }

        let remaining = packet_len - 3;
        let mut full = vec![0u8; packet_len];
        full[0] = header[0];
        full[1] = header[1];
        full[2] = header[2];

        if remaining > 0 {
            self.socket.read_exact(&mut full[3..]).ok()?;
        }

        log::info!("Read packet: code={:#X} len={}", full[0], packet_len);
        Some(full)
    }

    pub fn setpath(&mut self, p: &str) -> bool {
        let p = SetpathDirection::Child(p.to_string()).build(Some(self.message_handle));
        self.socket.write_all(&p);
        self.socket.flush();
        if let Some(buf) = self.read_obex_packet() {
            log::info!("READ DATA {:?} BYTES {:x?}", buf.len(), buf);
            true
        } else {
            false
        }
    }

    pub fn try_get_messages(&mut self) -> bool {
        let req = MapGetMessagesListing {
            connection_id: self.message_handle,
            max_list_count: 0, // just get the count, no XML body
            list_start_offset: 0,
            filter_message_type: 0x00,
            filter_read_status: 0x00,
            subject_length: None,
            filter_period_begin: None,
            filter_period_end: None,
        };
        let p = req.serialize();
        self.socket.write_all(&p).ok();
        self.socket.flush().ok();
        if let Some(buf) = self.read_obex_packet() {
            let code = buf[0];
            log::info!("get_messages response: {:#X}", code);
            return (code & 0x7F) == 0x20;
        }
        false
    }

    pub fn set_root(&mut self) -> bool {
        log::info!("SETPATH -> root (empty name)");
        let mut pkt = vec![0x85, 0x00, 0x00]; // SETPATH | Final
        pkt.push(0x00); // flags = 0x00 (not backup)
        pkt.push(0x00); // constants

        // Connection-ID
        pkt.push(0xCB);
        pkt.extend_from_slice(&self.message_handle.to_be_bytes());

        // Empty Name header — this triggers root navigation per Android source
        let empty_utf16 = [0x00u8, 0x00]; // just null terminator
        let name_len = (3 + empty_utf16.len()) as u16;
        pkt.push(0x01);
        pkt.extend_from_slice(&name_len.to_be_bytes());
        pkt.extend_from_slice(&empty_utf16);

        let total = pkt.len() as u16;
        pkt[1] = (total >> 8) as u8;
        pkt[2] = (total & 0xFF) as u8;

        self.socket.write_all(&pkt).ok();
        self.socket.flush().ok();

        if let Some(buf) = self.read_obex_packet() {
            let code = buf[0];
            log::info!("set_root response: {:#X}", code);
            (code & 0x7F) == 0x20
        } else {
            false
        }
    }

    pub fn go_to_root(&mut self) {
        // Send parent repeatedly until we get an error (means we're at root)
        for _ in 0..10 {
            let pkt = SetpathDirection::Parent.build(Some(self.message_handle));
            self.socket.write_all(&pkt).ok();
            self.socket.flush().ok();
            if let Some(buf) = self.read_obex_packet() {
                let code = buf[0];
                log::info!("go_to_root parent step: {:#X}", code);
                if (code & 0x7F) != 0x20 {
                    break; // hit the top
                }
            }
        }
    }

    pub fn get_folder_listing(&mut self) -> String {
        let p = build_get_folder_listing(self.message_handle);
        log::info!("Packet to list folder: {:x?}", p);
        self.socket.write_all(&p).ok();
        self.socket.flush().ok();
        let mut buf = [0u8; 4096];
        if let Some(buf) = self.read_obex_packet() {
            log::info!("FOLDER LISTING RESPONSE CODE: {:#X}", buf[0]);
            let xml = extract_body(&buf);
            log::info!("FOLDER LISTING XML:\n{}", xml);
            return xml;
        }
        String::new()
    }

    pub fn get_messages(&mut self) {
        let req = MapGetMessagesListing {
            connection_id: self.message_handle,
            max_list_count: 127,
            list_start_offset: 0,
            filter_message_type: 0x00,
            filter_read_status: 0x00,
            subject_length: Some(50),
            filter_period_begin: None,
            filter_period_end: None,
        };

        self.socket.write_all(&req.serialize()).ok();
        self.socket.flush().ok();

        let mut full_body: Vec<u8> = Vec::new();
        let mut app_params_out: Option<MapListingAppParams> = None;
        let mut first = true;

        loop {
            let data = match self.read_obex_packet() {
                Some(d) => d,
                None => {
                    log::error!("Failed to read packet");
                    break;
                }
            };

            let response_code = data[0];
            let is_final = response_code == 0xA0;
            let is_continue = response_code == 0x90;

            log::info!(
                "Packet code={:#X} total_len={} final={} continue={}",
                response_code,
                data.len(),
                is_final,
                is_continue
            );

            // Walk headers
            let mut pos = 3;
            while pos < data.len() {
                let header_id = data[pos];
                pos += 1;

                match header_id {
                    0xCB => {
                        if pos + 4 > data.len() {
                            break;
                        }
                        pos += 4;
                    }
                    0x4C => {
                        if pos + 2 > data.len() {
                            break;
                        }
                        let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                        pos += 2;
                        let end = (pos + len - 3).min(data.len());
                        if first {
                            app_params_out = Some(parse_map_listing_app_params(&data[pos..end]));
                        }
                        pos = end;
                    }
                    0x48 | 0x49 => {
                        if pos + 2 > data.len() {
                            break;
                        }
                        let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                        pos += 2;
                        let data_len = len.saturating_sub(3);
                        let end = (pos + data_len).min(data.len());
                        full_body.extend_from_slice(&data[pos..end]);
                        pos = end;
                    }
                    id if (id & 0xC0) == 0xC0 => {
                        if pos + 4 > data.len() {
                            break;
                        }
                        pos += 4;
                    }
                    id if (id & 0xC0) == 0x40 || (id & 0xC0) == 0x00 => {
                        if pos + 2 > data.len() {
                            break;
                        }
                        let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                        if len < 3 {
                            break;
                        }
                        pos += len - 1;
                    }
                    id if (id & 0xC0) == 0x80 => {
                        if pos + 1 > data.len() {
                            break;
                        }
                        pos += 1;
                    }
                    _ => break,
                }
            }

            first = false;

            if is_final {
                log::info!("Final packet, done.");
                break;
            }

            if is_continue {
                // Send empty GET to pull next chunk
                let cont = self.build_get_continue();
                self.socket.write_all(&cont).ok();
                self.socket.flush().ok();
            } else {
                log::error!("Unexpected code {:#X}", response_code);
                break;
            }
        }

        let xml = String::from_utf8(full_body).unwrap_or_default();
        let messages = parse_msg_listing_xml(&xml);

        log::info!(
            "XML length: {} bytes, messages: {}",
            xml.len(),
            messages.len()
        );
        for msg in &messages {
            log::info!("msg: {:#?}", msg);
        }
    }

    /// Empty GET request to continue a multi-packet response
    fn build_get_continue(&self) -> Vec<u8> {
        let mut pkt = vec![0x83, 0x00, 0x00]; // GET | Final
        pkt.push(0xCB);
        pkt.extend_from_slice(&self.message_handle.to_be_bytes());
        let total = pkt.len() as u16;
        pkt[1] = (total >> 8) as u8;
        pkt[2] = (total & 0xFF) as u8;
        pkt
    }

    fn build_put_final(connection_id: u32) -> Vec<u8> {
        let mut pkt = vec![0x82, 0x00, 0x00]; // PUT | Final

        pkt.push(0xCB);
        pkt.extend_from_slice(&connection_id.to_be_bytes());

        // empty end-of-body
        pkt.push(0x49);
        pkt.extend_from_slice(&[0x00, 0x03]);

        let total = pkt.len() as u16;
        pkt[1] = (total >> 8) as u8;
        pkt[2] = (total & 0xFF) as u8;

        pkt
    }

    pub fn register_notification(&mut self, _mns_channel: u8) -> bool {
        // The OBEX spec requires a NUL-terminated string for the TYPE header.
        // Android's ObexHelper reads (length - 3) bytes and then strips the
        // trailing NUL before storing the Java String, so the comparison with
        // TYPE_SET_NOTIFICATION_REGISTRATION succeeds.
        let type_str = b"x-bt/MAP-NotificationRegistration\0";

        // Only NotificationStatus (tag 0x0E) is required; MASInstanceID
        // (0x0F) is not needed and some Android versions reject it.
        let app_params = [0x0E, 0x01, 0x01]; // NotificationStatus = ON

        let mut pkt = vec![0x02, 0x00, 0x00]; // PUT

        // ConnectionId header
        pkt.push(0xCB);
        pkt.extend_from_slice(&self.message_handle.to_be_bytes());

        // Type header (0x42)
        let type_len = (3 + type_str.len()) as u16;
        pkt.push(0x42);
        pkt.extend_from_slice(&type_len.to_be_bytes());
        pkt.extend_from_slice(type_str);

        // Application Parameters header (0x4C)
        let app_len = (3 + app_params.len()) as u16;
        pkt.push(0x4C);
        pkt.extend_from_slice(&app_len.to_be_bytes());
        pkt.extend_from_slice(&app_params);

        let total = pkt.len() as u16;
        pkt[1] = (total >> 8) as u8;
        pkt[2] = (total & 0xFF) as u8;

        log::info!("NotificationRegistration: {:02x?}", pkt);
        self.socket.write_all(&pkt).ok();
        self.socket.flush().ok();

        let data = self.read_obex_packet();

        if let Some(data) = data {
            if data[0] == 0x90 {
                let final_put = vec![
                    0x82, 0x00, 0x07, 0x49, 0x00, 0x04, 0x30, // required filler byte
                ];
                log::info!("Final put: {:x?}", final_put);
                self.socket.write_all(&final_put).ok();
                self.socket.flush().ok();

                let final_resp = self.read_obex_packet();
                if let Some(resp) = final_resp {
                    log::info!("final notification response: {:x?}", resp);
                    return (resp[0] & 0x7F) == 0x20;
                }
            }

            return (data[0] & 0x7F) == 0x20;
        }
        false
    }
}

fn try_map_connect(dev: &mut BluetoothDevice, channel: u8) -> Result<BluetoothSocket, String> {
    if let Ok(mut socket) = dev.get_rfcomm_socket(channel, true) {
        match socket.connect() {
            Ok(_) => {
                log::info!("Got a socket");
                Ok(socket)
            }
            Err(e) => Err(format!("Socket connect not good {}", e)),
        }
    } else {
        Err("Failed to get rfcomm socket".to_string())
    }
}

pub struct MnsServer {
    profile: bluetooth_rust::BluetoothRfcommProfileAsync,
}

impl MnsServer {
    pub async fn new(
        adapter: &bluetooth_rust::BluetoothAdapter,
        channel: u16,
    ) -> Result<Self, String> {
        // Build a proper MAP MNS SDP record so the phone can discover the MNS
        // service via SDP and know which RFCOMM channel to connect back to.
        // The ProtocolDescriptorList MUST include OBEX (0x0008); without it
        // Android does not recognise the service as an OBEX/MAP endpoint and
        // will not attempt to connect.
        let sdp_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" ?>
<record>
  <attribute id="0x0001">
    <sequence>
      <uuid value="0x1133"/>
    </sequence>
  </attribute>
  <attribute id="0x0004">
    <sequence>
      <sequence>
        <uuid value="0x0100"/>
      </sequence>
      <sequence>
        <uuid value="0x0003"/>
        <uint8 value="{:#04X}"/>
      </sequence>
      <sequence>
        <uuid value="0x0008"/>
      </sequence>
    </sequence>
  </attribute>
  <attribute id="0x0009">
    <sequence>
      <sequence>
        <uuid value="0x1134"/>
        <uint16 value="0x0101"/>
      </sequence>
    </sequence>
  </attribute>
  <attribute id="0x0100">
    <text value="MAP MNS"/>
  </attribute>
</record>"#,
            channel
        );
        let psettings = bluetooth_rust::BluetoothRfcommProfileSettings {
            uuid: bluetooth_rust::BluetoothUuid::ObexMns.as_str().to_string(),
            name: Some("Obex Message Notification Service".to_string()),
            service_uuid: Some(bluetooth_rust::BluetoothUuid::ObexMns.as_str().to_string()),
            channel: Some(channel),
            psm: None,
            authenticate: Some(false),
            authorize: Some(false),
            auto_connect: Some(true),
            sdp_record: Some(sdp_xml),
            sdp_version: Some(0x0100),
            sdp_features: Some(0x001f),
        };
        if let Some(adapter) = adapter.supports_async() {
            let profile = adapter
                .register_rfcomm_profile(psettings)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Self { profile })
        } else {
            Err("Async not supported".to_string())
        }
    }

    pub async fn run(mut self) {
        use bluetooth_rust::BluetoothRfcommProfileAsyncTrait;

        loop {
            log::info!("MNS server waiting for phone to connect...");

            match self.profile.connectable().await {
                Ok(connectable) => {
                    log::info!("MNS: incoming connection request, accepting...");

                    match bluetooth_rust::BluetoothRfcommConnectableAsyncTrait::accept(connectable)
                        .await
                    {
                        Ok(stream) => {
                            log::info!("MNS: phone connected");

                            tokio::spawn(async move {
                                Self::handle_client(stream).await;
                            });
                        }
                        Err(e) => {
                            log::error!("MNS accept error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    log::error!("MNS connectable error (stopping): {}", e);
                    break;
                }
            }
        }
    }

    async fn handle_client(mut stream: bluetooth_rust::BluetoothStream) {
        use tokio::io::AsyncWriteExt;

        // ---- 1. Expect OBEX CONNECT ----
        let data = match Self::read_packet(&mut stream).await {
            Some(d) => d,
            None => return,
        };

        log::info!("MNS received packet: {:02x?}", data);

        // Validate CONNECT opcode
        if data.first() != Some(&0x80) {
            log::warn!("MNS: expected CONNECT, got {:02X?}", data.first());
            return;
        }

        // ---- 2. Reply to CONNECT ----
        let reply = Self::build_connect_reply();
        if let Err(e) = stream.write_all(&reply).await {
            log::warn!("MNS failed to send CONNECT reply: {e}");
            return;
        }
        let _ = stream.flush().await;

        log::info!("MNS: OBEX session established");

        // ---- 3. Main session loop ----
        loop {
            let data = match Self::read_packet(&mut stream).await {
                Some(d) => d,
                None => {
                    log::info!("MNS: phone disconnected");
                    break;
                }
            };

            let opcode = data.get(0).copied().unwrap_or(0);

            match opcode {
                0x81 => {
                    // DISCONNECT
                    log::info!("MNS: DISCONNECT received");
                    let _ = stream.write_all(&[0xA0, 0x00, 0x03]).await;
                    let _ = stream.flush().await;
                    break;
                }

                0x02 | 0x82 => {
                    log::info!("MNS event received");

                    let body = extract_body(&data);

                    log::info!("MAP EVENT:\n{}", body);

                    Self::parse_event(&body);

                    // ACK success
                    stream.write_all(&[0xA0, 0x00, 0x03]).await.ok();
                }

                _ => {
                    log::warn!("MNS: unknown opcode {:#X}", opcode);

                    // OBEX protocol-safe "Bad Request"
                    let _ = stream.write_all(&[0xC0, 0x00, 0x03]).await;
                    let _ = stream.flush().await;
                }
            }
        }

        log::info!("MNS: session ended");
    }

    async fn read_packet(stream: &mut bluetooth_rust::BluetoothStream) -> Option<Vec<u8>> {
        use tokio::io::AsyncReadExt;
        let mut header = [0u8; 3];
        stream.read_exact(&mut header).await.ok()?;
        let len = u16::from_be_bytes([header[1], header[2]]) as usize;
        if len < 3 {
            return None;
        }
        let mut full = vec![0u8; len];
        full[..3].copy_from_slice(&header);
        if len > 3 {
            stream.read_exact(&mut full[3..]).await.ok()?;
        }
        Some(full)
    }

    fn build_connect_reply() -> Vec<u8> {
        let who: [u8; 16] = [
            0xBB, 0x58, 0x2B, 0x41, 0x42, 0x0C, 0x11, 0xDB, 0xB0, 0xDE, 0x08, 0x00, 0x20, 0x0C,
            0x9A, 0x66,
        ];
        let mut reply = vec![
            0xA0, 0x00, 0x00, // OK + length placeholder
            0x10, // OBEX version
            0x00, // flags
            0x10, 0x00, // max packet size
        ];
        let who_len = (3 + who.len()) as u16;
        reply.push(0x4A); // WHO header
        reply.extend_from_slice(&who_len.to_be_bytes());
        reply.extend_from_slice(&who);
        let total = reply.len() as u16;
        reply[1] = (total >> 8) as u8;
        reply[2] = (total & 0xFF) as u8;
        reply
    }

    fn parse_event(xml: &str) {
        // <MAP-event-report version="1.0">
        //   <event type="NewMessage" handle="..." folder="..." msg_type="SMS_GSM"/>
        // </MAP-event-report>
        if let Some(start) = xml.find("<event ") {
            if let Some(end) = xml[start..].find("/>") {
                let tag = &xml[start..start + end + 2];
                let get = |attr: &str| -> Option<String> {
                    let needle = format!("{}=\"", attr);
                    let s = tag.find(&needle)? + needle.len();
                    let e = tag[s..].find('"')? + s;
                    Some(tag[s..e].to_string())
                };
                log::info!(
                    "MAP Event: type={:?} handle={:?} folder={:?} msg_type={:?}",
                    get("type"),
                    get("handle"),
                    get("folder"),
                    get("msg_type")
                );
            }
        }
    }
}

pub async fn start_mns(adapter: &bluetooth_rust::BluetoothAdapter, chan: u16) -> Result<(), String> {
    let mns = MnsServer::new(adapter, chan)
        .await?;
    tokio::spawn(async move { mns.run().await });
    Ok(())
}

pub async fn connect_to_mas(adapter: &bluetooth_rust::BluetoothAdapter, mut dev: bluetooth_rust::BluetoothDevice) -> Result<(), String> {
    match dev.get_uuids() {
        Ok(uuids) => {
            if !uuids.contains(&bluetooth_rust::BluetoothUuid::ObexMas) {
                return Err("No MAS service on device".to_string());
            }
            let channel =
                if let Ok(sdp) = dev.run_sdp(bluetooth_rust::BluetoothUuid::ObexMas) {
                    sdp.rfcomm_channel().unwrap_or(1)
                } else {
                    1
                };
            match try_map_connect(&mut dev, channel) {
                Ok(s) => {
                    log::info!("MAP socket on ch {} — processing immediately", channel);
                    let mut client = MessageClient::new(s);
                    if !client.has_session() {
                        log::warn!("OBEX session not established, skipping device");
                        return Err("Faild to establish obex session".to_string());
                    }
                    let not = client.register_notification(17);
                    tokio::task::yield_now().await;
                    log::info!("Registered for notifications : {}", not);
                    if !not {
                        // Keep going even on failure: some phones return 0xC0 but
                        // still process the registration internally and connect to
                        // the MNS server.  We log the failure but do not skip, so
                        // we can observe whether MNS gets a connection anyway.
                        log::warn!(
                            "Notification registration returned failure — \
                                proceeding anyway to see if MNS still connects"
                        );
                    }
                    //client.set_root();
                    //client.setpath("telecom");
                    //client.setpath("msg");
                    //client.setpath("inbox");
                    log::info!("Waiting for MNS callback...");
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                    //client.get_folder_listing();
                    //client.get_messages();
                }
                Err(e) => {
                    log::error!("Error trying to connect map: {}", e);
                }
            }
        }
        Err(e) => log::error!("Error getting uuids: {}", e),
    }
    Ok(())
}
