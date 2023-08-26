use crate::utils::data_dir;
use anyhow::anyhow;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine;
use directories::ProjectDirs;
use once_cell::sync::OnceCell;
use rustls::{Certificate, PrivateKey};
use serde_derive::{Deserialize, Serialize};

pub fn read_config() -> &'static Config {
    static UUID: OnceCell<Config> = OnceCell::new();
    UUID.get_or_init(|| {
        let path = data_dir().join("config");
        let config = std::fs::read(path).expect("Failed to read config");
        let config: Config = postcard::from_bytes(&config).expect("Failed to deserialize config");
        config
    })
}

pub fn generate_config() -> anyhow::Result<()> {
    let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm")
        .ok_or(anyhow!("Failed to get project dirs"))?;
    let path = dirs.data_dir();
    let path = path.join("config");
    let cert = rcgen::generate_simple_self_signed(vec!["swarm".into()])?;
    let config = Config {
        cert: Certificate(cert.serialize_der()?),
        key: PrivateKey(cert.serialize_private_key_der()),
        mqtt: "test.mosquitto.org".to_string(),
    };
    std::fs::write(path, postcard::to_allocvec(&config)?)?;
    Ok(())
}

pub fn serialize_config() -> anyhow::Result<String> {
    let config = read_config();
    let bin = postcard::to_allocvec(&config)?;
    let mut result = Vec::new();
    zstd::stream::copy_encode(bin.as_slice(), &mut result, 22)?;

    Ok(STANDARD_NO_PAD.encode(&result))
}

pub fn write_serialized_config(token: String) -> anyhow::Result<()> {
    let compressed = STANDARD_NO_PAD.decode(token.as_bytes())?;
    let mut decompressed = Vec::new();
    zstd::stream::copy_decode(&compressed[..], &mut decompressed)?;
    let config: Config = postcard::from_bytes(&decompressed)?;
    let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm")
        .ok_or(anyhow!("Failed to get project dirs"))?;
    let path = dirs.data_dir();
    let path = path.join("config");
    std::fs::write(path, postcard::to_allocvec(&config)?)?;

    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    #[serde(with = "CertificatedDef")]
    pub cert: Certificate,
    #[serde(with = "PrivateKeyDef")]
    pub key: PrivateKey,
    pub mqtt: String,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "Certificate")]
struct CertificatedDef(Vec<u8>);

#[derive(Serialize, Deserialize)]
#[serde(remote = "PrivateKey")]
struct PrivateKeyDef(Vec<u8>);
