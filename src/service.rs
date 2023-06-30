use service_manager::*;

pub fn install_service() {
    let label: ServiceLabel = "ffmpeg-swarm".parse().unwrap();
    let mut manager = <dyn ServiceManager>::native().expect("Failed to detect management platform");

    _ = manager.set_level(ServiceLevel::User);

    manager
        .install(ServiceInstallCtx {
            label: label.clone(),
            program: std::env::current_exe().unwrap(),
            args: vec![],
            contents: None,
        })
        .expect("Failed to install");

    println!("Service installed successfully");
}

pub fn uninstall_service() {
    let label: ServiceLabel = "ffmpeg-swarm".parse().unwrap();
    let mut manager = <dyn ServiceManager>::native().expect("Failed to detect management platform");

    _ = manager.set_level(ServiceLevel::User);

    manager
        .uninstall(ServiceUninstallCtx {
            label: label.clone(),
        })
        .expect("Failed to stop");

    println!("Service uninstalled successfully");
}
