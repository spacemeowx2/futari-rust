use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Relay server information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayServerInfo {
    pub address: String,
    pub port: u16,
}

/// Information about a mecha
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MechaInfo {
    pub is_join: bool,
    pub ip_address: u32, // IP address as UInt
    pub music_id: i32,
    pub entrys: Vec<bool>,
    pub user_ids: Vec<i64>,
    pub user_names: Vec<String>,
    pub icon_ids: Vec<i32>,
    pub fumen_difs: Vec<i32>,
    pub rateing: Vec<i32>,
    pub class_value: Vec<i32>,
    pub max_class_value: Vec<i32>,
    pub user_type: Vec<i32>,
}

/// Recruitment information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecruitInfo {
    pub mecha_info: MechaInfo,
    pub music_id: i32,
    pub group_id: i32,
    pub event_mode_id: bool,
    pub join_number: i32,
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
        let parts: Vec<&str> = input.split(',').collect();
        if parts.len() < 17 || parts[0] != "1" {
            return None;
        }

        let parse_uint = |s: &str| -> Option<u32> {
            if s.is_empty() {
                None
            } else {
                s.parse::<u32>().ok()
            }
        };

        Some(Msg {
            cmd: parse_uint(parts[1])?,
            proto: parse_uint(parts[2]),
            sid: parse_uint(parts[3]),
            src: parse_uint(parts[4]),
            s_port: parse_uint(parts[5]),
            dst: parse_uint(parts[6]),
            d_port: parse_uint(parts[7]),
            data: if parts[16].is_empty() {
                None
            } else {
                Some(parts[16].to_string())
            },
        })
    }

    /// Convert the message to a string for transmission
    pub fn to_string(&self) -> String {
        let fmt_uint =
            |opt: &Option<u32>| -> String { opt.map_or(String::new(), |val| val.to_string()) };

        format!(
            "1,{},{},{},{},{},{},{},,,,,,,,{},",
            self.cmd,
            fmt_uint(&self.proto),
            fmt_uint(&self.sid),
            fmt_uint(&self.src),
            fmt_uint(&self.s_port),
            fmt_uint(&self.dst),
            fmt_uint(&self.d_port),
            self.data.as_deref().unwrap_or("")
        )
    }
}

/// Command constants for the relay protocol
pub mod commands {
    // Control plane commands
    pub const CTL_START: u32 = 1;
    pub const CTL_BIND: u32 = 2;
    pub const CTL_HEARTBEAT: u32 = 3;
    pub const CTL_TCP_CONNECT: u32 = 4;
    pub const CTL_TCP_ACCEPT: u32 = 5;
    pub const CTL_TCP_ACCEPT_ACK: u32 = 6;
    pub const CTL_TCP_CLOSE: u32 = 7;

    // Data plane commands
    pub const DATA_SEND: u32 = 21;
    pub const DATA_BROADCAST: u32 = 22;
}

/// Protocol types
pub mod protocols {
    pub const TCP: u32 = 6;
    pub const UDP: u32 = 17;
}

/// Utility functions
pub fn current_time_millis() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(_) => 0,
    }
}
