pub fn get_wifi_adapters() -> Result<Vec<String>, String> {
    let mut wifis = Vec::new();
    let output = std::process::Command::new("nmcli")
        .args(&[
            "-t",
            "device",
            "status"])
        .output().map_err(|e| e.to_string())?;
    let s : String = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    for entry in s.lines() {
        if entry.contains(":wifi:") {
            let n : Vec<&str> = entry.split(':').collect();
            wifis.push(n[0].to_string());
        }
    }
    log::error!("Wifi adapter output: {:?}", s);
    Ok(wifis)
}