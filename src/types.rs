use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// Constants
pub const PROTO_VERSION: &str = "1";

/// Relay server information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayServerInfo {
    pub name: String,
    pub addr: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub official: bool,
}

fn default_port() -> u16 {
    20101
}

fn default_true() -> bool {
    true
}

/// Information about a mecha
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MechaInfo {
    #[serde(rename = "IsJoin")]
    pub is_join: bool,
    #[serde(rename = "IpAddress")]
    pub ip_address: u32, // IP address as UInt
    #[serde(rename = "MusicID")]
    pub music_id: i32,
    #[serde(rename = "Entrys")]
    pub entrys: Vec<bool>,
    #[serde(rename = "UserIDs")]
    pub user_ids: Vec<i64>,
    #[serde(rename = "UserNames")]
    pub user_names: Vec<String>,
    #[serde(rename = "IconIDs")]
    pub icon_ids: Vec<i32>,
    #[serde(rename = "FumenDifs")]
    pub fumen_difs: Vec<i32>,
    #[serde(rename = "Rateing")]
    pub rateing: Vec<i32>,
    #[serde(rename = "ClassValue")]
    pub class_value: Vec<i32>,
    #[serde(rename = "MaxClassValue")]
    pub max_class_value: Vec<i32>,
    #[serde(rename = "UserType")]
    pub user_type: Vec<i32>,
}

/// Recruitment information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecruitInfo {
    #[serde(rename = "MechaInfo")]
    pub mecha_info: MechaInfo,
    #[serde(rename = "MusicID")]
    pub music_id: i32,
    #[serde(rename = "GroupID")]
    pub group_id: i32,
    #[serde(rename = "EventModeID")]
    pub event_mode_id: bool,
    #[serde(rename = "JoinNumber")]
    pub join_number: i32,
    #[serde(rename = "PartyStance")]
    pub party_stance: i32,
    #[serde(rename = "_startTimeTicks")]
    pub start_time_ticks: i64,
    #[serde(rename = "_recvTimeTicks")]
    pub recv_time_ticks: i64,
}

/// Recruitment record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecruitRecord {
    #[serde(rename = "RecruitInfo")]
    pub recruit_info: RecruitInfo,
    #[serde(rename = "Keychip")]
    pub keychip: String,
    #[serde(rename = "Server")]
    pub server: Option<RelayServerInfo>,
    #[serde(rename = "Time")]
    #[serde(default)]
    pub time: i64,
}

/// Message structure for relay communication
#[derive(Debug, Clone)]
pub struct Msg {
    pub cmd: u32,
    pub proto: Option<u32>,
    pub sid: Option<u32>,
    pub src: Option<u32>,
    pub s_port: Option<u32>,
    pub dst: Option<u32>,
    pub d_port: Option<u32>,
    pub data: Option<String>,
}

impl Msg {
    pub fn new(cmd: u32) -> Self {
        Self {
            cmd,
            proto: None,
            sid: None,
            src: None,
            s_port: None,
            dst: None,
            d_port: None,
            data: None,
        }
    }

    /// Parse a message from a comma-delimited string
    pub fn parse(input: &str) -> Option<Self> {
        // Ignore the newline character at the end
        let input = input.trim_end_matches('\n');
        let parts: Vec<&str> = input.split(',').collect();
        // Check only that the first part is "1" (protocol identifier)
        // and we have at least a command part (index 1)
        if parts.len() < 2 || parts[0] != PROTO_VERSION {
            return None;
        }

        // Create a new message with default command 0
        // We'll update the command and other fields below
        let mut msg = Msg::new(0);

        // Parse the command
        if let Ok(cmd) = parts.get(1).unwrap_or(&"").parse::<u32>() {
            msg.cmd = cmd;
        } else {
            return None;
        }

        // Helper function to parse fields
        let parse_field = |index: usize| {
            parts.get(index).and_then(|s| {
                if s.is_empty() {
                    None
                } else {
                    s.parse::<u32>().ok()
                }
            })
        };

        // Set fields in order similar to Kotlin implementation
        // Only try to parse fields if they exist in the input
        msg.proto = parse_field(2);
        msg.sid = parse_field(3);
        msg.src = parse_field(4);
        msg.s_port = parse_field(5);
        msg.dst = parse_field(6);
        msg.d_port = parse_field(7);

        // Parse the data field (index 16), if it exists
        if parts.len() > 16 {
            let data_value = parts[16];
            if !data_value.is_empty() {
                msg.data = Some(data_value.to_string());
            }
        }

        Some(msg)
    }

    /// Convert the message to a string for transmission
    pub fn to_string(&self) -> String {
        // Helper function to format optional u32 values
        let to_str =
            |opt: &Option<u32>| -> String { opt.map_or(String::new(), |val| val.to_string()) };

        // Format according to the Kotlin implementation
        format!(
            "{},{},{},{},{},{},{},{},,,,,,,,{},",
            PROTO_VERSION,
            self.cmd,
            to_str(&self.proto),
            to_str(&self.sid),
            to_str(&self.src),
            to_str(&self.s_port),
            to_str(&self.dst),
            to_str(&self.d_port),
            self.data.as_deref().unwrap_or("")
        )
    }
}

/// Command constants for the relay protocol
pub mod commands {
    // Control plane commands
    pub const CTL_START: u32 = 1;
    #[allow(dead_code)]
    pub const CTL_BIND: u32 = 2;
    pub const CTL_HEARTBEAT: u32 = 3;
    pub const CTL_TCP_CONNECT: u32 = 4; // Accept a new multiplexed TCP stream
    pub const CTL_TCP_ACCEPT: u32 = 5;
    #[allow(dead_code)]
    pub const CTL_TCP_ACCEPT_ACK: u32 = 6;
    pub const CTL_TCP_CLOSE: u32 = 7;

    // Data plane commands
    pub const DATA_SEND: u32 = 21;
    pub const DATA_BROADCAST: u32 = 22;
}

/// Protocol types
pub mod protocols {
    #[allow(dead_code)]
    pub const TCP: u32 = 6;
    pub const UDP: u32 = 17;
}

/// Convert a keychip ID to a stub IP (virtual IP address)
pub fn keychip_to_stub_ip(keychip: &str) -> u32 {
    let mut hasher = Md5::new();
    hasher.update(keychip);
    let result = hasher.finalize();

    // Convert first 4 bytes of MD5 hash to u32 (same as Kotlin implementation)
    ((result[0] as u32 & 0xFF) << 24)
        | ((result[1] as u32 & 0xFF) << 16)
        | ((result[2] as u32 & 0xFF) << 8)
        | (result[3] as u32 & 0xFF)
}

/// Utility functions
pub fn current_time_millis() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_heartbeat() {
        // Test parsing a simple heartbeat message "1,3"
        let msg = Msg::parse("1,3");
        assert!(msg.is_some(), "Failed to parse heartbeat message");

        let msg = msg.unwrap();
        assert_eq!(msg.cmd, 3, "Command should be 3 (heartbeat)");
        assert!(msg.proto.is_none(), "Proto should be None");
        assert!(msg.sid.is_none(), "Session ID should be None");
        assert!(msg.src.is_none(), "Source should be None");
        assert!(msg.s_port.is_none(), "Source port should be None");
        assert!(msg.dst.is_none(), "Destination should be None");
        assert!(msg.d_port.is_none(), "Destination port should be None");
        assert!(msg.data.is_none(), "Data should be None");
    }
}
