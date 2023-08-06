use rustls::{Certificate, PrivateKey};
use directories::ProjectDirs;
use uuid::Uuid;
use once_cell::sync::OnceCell;

pub fn read_or_generate_certs() -> anyhow::Result<(Certificate, PrivateKey)> {
    let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm").unwrap();
    let path = dirs.data_dir();
    std::fs::create_dir_all(&path)?;
    let cert_path = path.join("cert.der");
    let key_path = path.join("key.der");

    if cert_path.exists() && key_path.exists() {
        let cert = std::fs::read(cert_path)?;
        let key = std::fs::read(key_path)?;
        Ok((Certificate(cert), PrivateKey(key)))
    } else {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let key = cert.serialize_private_key_der();
        let cert = cert.serialize_der().unwrap();
        std::fs::write(cert_path, &cert)?;
        std::fs::write(key_path, &key)?;
        Ok((Certificate(cert), PrivateKey(key)))
    }
}

pub fn read_or_generate_uuid() -> anyhow::Result<&'static Uuid> {
    static UUID: OnceCell<Uuid> = OnceCell::new();
    UUID.get_or_try_init(|| {
        let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm").unwrap();
        let path = dirs.data_dir();
        std::fs::create_dir_all(&path)?;
        let uuid_path = path.join("uuid");

        if uuid_path.exists() {
            let uuid = std::fs::read_to_string(uuid_path)?;
            Ok(Uuid::parse_str(&uuid)?)
        } else {
            let uuid = Uuid::new_v4();
            std::fs::write(uuid_path, uuid.to_string())?;
            Ok(uuid)
        }
    })
}