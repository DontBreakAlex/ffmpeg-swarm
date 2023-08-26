use directories::ProjectDirs;
use once_cell::sync::OnceCell;
use std::path::Path;
use uuid::Uuid;

pub fn read_or_generate_uuid() -> anyhow::Result<&'static Uuid> {
    static UUID: OnceCell<Uuid> = OnceCell::new();
    UUID.get_or_try_init(|| {
        let path = data_dir();
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

pub fn runtime_dir() -> &'static Path {
    static RUNTIME_DIR: OnceCell<Box<Path>> = OnceCell::new();
    RUNTIME_DIR.get_or_init(|| {
        let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm").unwrap();
        let path = dirs.runtime_dir().unwrap();
        std::fs::create_dir_all(&path).unwrap();
        path.into()
    })
}

pub fn data_dir() -> &'static Path {
    static DATA_DIR: OnceCell<Box<Path>> = OnceCell::new();
    DATA_DIR.get_or_init(|| {
        let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm").unwrap();
        let path = dirs.data_dir();
        std::fs::create_dir_all(&path).unwrap();
        path.into()
    })
}
