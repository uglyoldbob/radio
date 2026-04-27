//! Code for bluetooth messages from bluetooth devices

/// The type of a message that can be received
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum MessageType {
    /// An email message
    #[default]
    Email,
    /// text of some variety
    SmsGsm,
    /// text of some variety
    SmsCdma,
    /// media of some variety
    Mms,
}

impl TryFrom<&str> for MessageType {
    type Error = String;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(match value {
            "EMAIL" => Self::Email,
            "SMS_GSM" => Self::SmsGsm,
            "SMS_CDMA" => Self::SmsCdma,
            "MMS" => Self::Mms,
            _ => {
                return Err("Invalid message type".to_string());
            }
        })
    }
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
    /// Attempt to parse the given string to a Self
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

/// A struct representing a contact for sending or receiving a message
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
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
    /// Attempt to parse a Self from the given Lines object
    pub fn parse(c: &mut std::io::Lines<std::io::Cursor<&str>>) -> Result<Self, String> {
        let mut out = Self::default();
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

/// The content of a message
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BContent {
    part: Option<String>,
    encoding: Option<String>,
    charset: Option<String>,
    language: Option<String>,
    length: usize,
    message: String,
}

impl BContent {
    /// Attempt to parse a Self from the given Lines object
    pub fn parse(lines: &mut std::io::Lines<std::io::Cursor<&str>>) -> Result<Self, String> {
        let mut content = BContent::default();

        // Parse BBODY headers first
        loop {
            let line = lines.next()
                .ok_or("Unexpected end of input in BBODY")?
                .map_err(|e| e.to_string())?;
            let line = line.trim();

            if line == "BEGIN:MSG" {
                break;
            } else if line == "END:BBODY" {
                return Err("END:BBODY before BEGIN:MSG".to_string());
            } else if let Some(val) = line.strip_prefix("PART:") {
                content.part = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("ENCODING:") {
                content.encoding = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("CHARSET:") {
                content.charset = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("LANGUAGE:") {
                content.language = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("LENGTH:") {
                content.length = val.trim().parse::<usize>().map_err(|_| "Invalid LENGTH".to_string())?;
            }
            // unknown headers silently skipped
        }

        // Now collect message body lines until END:MSG,
        // using LENGTH as the source of truth if available.
        let mut body_lines: Vec<String> = Vec::new();

        if content.length > 0 {
            // Collect exactly `length` bytes worth of content.
            // The LENGTH field in the spec counts from the first byte after BEGIN:MSG\n
            // up to and including the END:MSG\r\n terminator.
            // We accumulate lines until our byte count reaches or exceeds LENGTH.
            let suffix = b"END:MSG";
            let mut byte_count = 0;

            loop {
                let line = lines.next()
                    .ok_or("Unexpected end of input reading MSG body")?
                    .map_err(|e| e.to_string())?;

                // +1 for the newline that was consumed
                byte_count += line.len() + 1;

                if line.trim() == "END:MSG" || byte_count >= content.length {
                    // This line is the real END:MSG terminator (or we hit the length
                    // boundary). Don't include it in the body.
                    break;
                }

                // If the line happens to contain "END:MSG" but we haven't hit
                // the length boundary yet, it's part of the message body.
                body_lines.push(line);
            }

            // Sanity check: if the last body line is END:MSG we over-collected
            if body_lines.last().map(|l| l.trim()) == Some("END:MSG") {
                body_lines.pop();
            }
        } else {
            // No LENGTH — fall back to rfind strategy: collect everything,
            // then trim from the last END:MSG backwards.
            let mut all_lines: Vec<String> = Vec::new();

            loop {
                let line = lines.next()
                    .ok_or("Unexpected end of input reading MSG body")?
                    .map_err(|e| e.to_string())?;

                if line.trim() == "END:MSG" {
                    // Keep consuming to find if there's another END:MSG
                    // (we can't distinguish body from terminator without LENGTH)
                    all_lines.push(line);
                    // Peek ahead: if the next line is END:BBODY or END:BENV we're done
                    // Since we can't peek a Lines iterator, we settle for rfind below.
                    break;
                }
                all_lines.push(line);
            }

            // Find the last END:MSG and treat everything before it as body
            let last_end = all_lines.iter().rposition(|l| l.trim() == "END:MSG");
            let body_end = last_end.unwrap_or(all_lines.len());
            body_lines = all_lines[..body_end].to_vec();
        }

        content.message = body_lines.join("\n");
        Ok(content)
    }
}

/// The envelope of a message, it might be recursive
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum BEnvelope {
    /// A nested object
    Envelope(Box<BEnvelope>),
    /// The actual message content
    Content(BContent),
}

impl BEnvelope {
    /// Attempt to parse a Self from the given Lines object
    pub fn parse(c: &mut std::io::Lines<std::io::Cursor<&str>>) -> Result<Self, String> {
        if let Some(Ok(line)) = c.next() {
            if line.as_str() == "BEGIN:BENV" {
                if let Ok(benv) = BEnvelope::parse(c) {
                    return Ok(Self::Envelope(Box::new(benv)));
                }
            }
            if line.as_str() == "BEGIN:BBODY" {
                if let Ok(benv) = BContent::parse(c) {
                    return Ok(Self::Content(benv));
                }
            }
        }
        Err("Unexpected value for envelope".to_string())
    }
}

/// A message received over bluetooth from a device
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BMessage {
    version: String,
    status_read: bool,
    mtype: MessageType,
    folder: String,
    originator: Vec<VCard>,
    message: Option<BEnvelope>,
}

fn last_512(s: &str) -> String {
    let len = s.chars().count();

    s.chars().skip(len.saturating_sub(512)).collect()
}

impl BMessage {
    /// Attempt to parse a Self from the given Lines object
    pub fn parse(c: &mut std::io::Lines<std::io::Cursor<&str>>) -> Result<Self, String> {
        let mut out = Self::default();
        if let Some(Ok(line)) = c.next() {
            if line.as_str() != "BEGIN:BMSG" {
                return Err("No begin line found".to_string());
            }
        } else {
            return Err("No begin line found".to_string());
        }
        loop {
            if let Some(Ok(line)) = c.next() {
                if line.as_str() == "END:BMSG" {
                    break;
                }
                if line.starts_with("VERSION:") {
                    if let Some(v) = line.split_once(":") {
                        out.version = v.1.to_string();
                    }
                }
                if line.starts_with("STATUS:") {
                    out.status_read = if let Some(v) = line.split_once(":") {
                        match v.1 {
                            "UNREAD" => false,
                            "READ" => true,
                            _ => {
                                return Err(format!("Invalid message status {}", v.1));
                            }
                        }
                    } else {
                        return Err("Invalid message status line".to_string());
                    };
                }
                if line.starts_with("TYPE:") {
                    if let Some(v) = line.split_once(":") {
                        out.mtype = v.1.try_into()?;
                    } else {
                        return Err("Invalid message line".to_string());
                    }
                }
                if line.starts_with("FOLDER:") {
                    if let Some(v) = line.split_once(":") {
                        out.folder = v.1.to_string();
                    }
                }
                if line.as_str() == "BEGIN:VCARD" {
                    if let Ok(v) = VCard::parse(c) {
                        out.originator.push(v);
                    }
                }
                if line.as_str() == "BEGIN:BENV" {
                    if let Ok(benv) = BEnvelope::parse(c) {
                        out.message = Some(benv);
                    }
                }
            } else {
                return Err("Not enough lines found".to_string());
            }
        }
        Ok(out)
    }
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