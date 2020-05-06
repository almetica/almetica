use serde::Serialize;
use std::net::Ipv4Addr;

#[derive(Serialize)]
pub struct ServerListEntry {
    pub id: i64,
    pub category: String,
    pub raw_name: String,
    pub name: String,
    pub crowdness: String,
    pub open: String,
    pub ip: Ipv4Addr,
    pub port: u16,
    pub lang: u16,
    pub popup: String,
}

#[derive(Serialize)]
pub struct ServerListResponse {
    pub servers: Vec<ServerListEntry>,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub ticket: String, // base64 encoded 128 bit token
}
