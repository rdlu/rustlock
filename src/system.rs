use log::{debug, error};
use mpris::PlayerFinder;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use zbus::Connection;

#[derive(Clone, Default, PartialEq)]
pub struct SystemStatus {
    pub battery_percent: Option<f64>,
    pub is_charging: bool,
    pub media_title: Option<String>,
    pub media_artist: Option<String>,
    pub media_playing: bool,
    pub media_art_url: Option<String>,
    pub media_art_data: Option<Arc<Vec<u8>>>,
    pub wifi_ssid: Option<String>,
    pub wifi_strength: Option<u8>,
    pub bluetooth_connected: bool,
    pub bluetooth_devices: Vec<String>,
    pub keyboard_layout: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum BackendCommand {
    PowerOff,
    Reboot,
    Suspend,
    MediaPlayPause,
    MediaStop,
    MediaNext,
    MediaPrev,
}

pub struct SystemManager {
    status: Arc<Mutex<SystemStatus>>,
    cmd_tx: mpsc::UnboundedSender<BackendCommand>,
}

impl SystemManager {
    pub fn new(config: &crate::config::Config) -> Self {
        let poll_interval = tokio::time::Duration::from_secs(config.system_poll_interval);
        let reconnect_delay = tokio::time::Duration::from_secs(config.dbus_reconnect_delay);
        let command_timeout = tokio::time::Duration::from_secs(config.command_timeout);

        let status = Arc::new(Mutex::new(SystemStatus::default()));
        let s_clone = status.clone();
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<BackendCommand>();

        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    error!("Failed to create tokio runtime for SystemManager: {}", e);
                    return;
                }
            };

            rt.block_on(async {
                let mut conn: Option<Connection> = None;
                let mut interval = tokio::time::interval(poll_interval);
                let mut last_art_url: Option<String> = None;
                let mut last_art_data: Option<Arc<Vec<u8>>> = None;

                loop {
                    if conn.is_none() {
                        match Connection::system().await {
                            Ok(c) => conn = Some(c),
                            Err(e) => {
                                error!("Failed to connect to system DBus: {}", e);
                                tokio::time::sleep(reconnect_delay).await;
                                interval = tokio::time::interval(poll_interval);
                                continue;
                            }
                        }
                    }

                    tokio::select! {
                        _ = interval.tick() => {
                            let mut new_status = SystemStatus::default();

                            if let Some(ref c) = conn {
                                if let Ok(reply) = c.call_method(
                                    Some("org.freedesktop.UPower"),
                                    "/org/freedesktop/UPower/devices/DisplayDevice",
                                    Some("org.freedesktop.DBus.Properties"),
                                    "GetAll",
                                    &("org.freedesktop.UPower.Device"),
                                ).await {
                                    use std::collections::HashMap;
                                    if let Ok(props) = reply.body().deserialize::<HashMap<String, zbus::zvariant::OwnedValue>>() {
                                        if let Some(v) = props.get("Percentage") {
                                            if let Ok(val) = v.downcast_ref::<f64>() {
                                                new_status.battery_percent = Some(val);
                                            }
                                        }
                                        if let Some(v) = props.get("State") {
                                            if let Ok(state) = v.downcast_ref::<u32>() {
                                                new_status.is_charging = state == 1;
                                            }
                                        }
                                    }
                                }

                                if let Ok(reply) = c.call_method(
                                    Some("org.freedesktop.NetworkManager"),
                                    "/org/freedesktop/NetworkManager",
                                    Some("org.freedesktop.NetworkManager"),
                                    "GetDevices",
                                    &(),
                                ).await {
                                    if let Ok(devices) = reply.body().deserialize::<Vec<zbus::zvariant::OwnedObjectPath>>() {
                                        for dev_path in devices {
                                            if let Ok(dev_type_reply) = c.call_method(
                                                Some("org.freedesktop.NetworkManager"),
                                                &dev_path,
                                                Some("org.freedesktop.DBus.Properties"),
                                                "Get",
                                                &("org.freedesktop.NetworkManager.Device", "DeviceType"),
                                            ).await {
                                                if let Ok(val) = dev_type_reply.body().deserialize::<zbus::zvariant::OwnedValue>() {
                                                    if let Ok(dev_type) = val.downcast_ref::<u32>() {
                                                        if dev_type == 2 {
                                                            if let Ok(active_ap_reply) = c.call_method(
                                                                Some("org.freedesktop.NetworkManager"),
                                                                &dev_path,
                                                                Some("org.freedesktop.DBus.Properties"),
                                                                "Get",
                                                                &("org.freedesktop.NetworkManager.Device.Wireless", "ActiveAccessPoint"),
                                                            ).await {
                                                                if let Ok(ap_val) = active_ap_reply.body().deserialize::<zbus::zvariant::OwnedValue>() {
                                                                    if let Ok(ap_path) = ap_val.downcast_ref::<zbus::zvariant::ObjectPath>() {
                                                                        if ap_path.as_str() != "/" {
                                                                            if let Ok(ssid_reply) = c.call_method(
                                                                                Some("org.freedesktop.NetworkManager"),
                                                                                &ap_path,
                                                                                Some("org.freedesktop.DBus.Properties"),
                                                                                "Get",
                                                                                &("org.freedesktop.NetworkManager.AccessPoint", "Ssid"),
                                                                            ).await {
                                                                                if let Ok(ssid_val) = ssid_reply.body().deserialize::<zbus::zvariant::OwnedValue>() {
                                                                                    let ssid_bytes: Result<Vec<u8>, _> = ssid_val.try_into();
                                                                                    if let Ok(ssid_bytes) = ssid_bytes {
                                                                                        new_status.wifi_ssid = Some(String::from_utf8_lossy(&ssid_bytes).to_string());
                                                                                    }
                                                                                }
                                                                            }
                                                                            if let Ok(strength_reply) = c.call_method(
                                                                                Some("org.freedesktop.NetworkManager"),
                                                                                &ap_path,
                                                                                Some("org.freedesktop.DBus.Properties"),
                                                                                "Get",
                                                                                &("org.freedesktop.NetworkManager.AccessPoint", "Strength"),
                                                                            ).await {
                                                                                if let Ok(strength_val) = strength_reply.body().deserialize::<zbus::zvariant::OwnedValue>() {
                                                                                    if let Ok(strength) = strength_val.downcast_ref::<u8>() {
                                                                                        new_status.wifi_strength = Some(strength);
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            break;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if let Ok(objects_reply) = c.call_method(
                                    Some("org.bluez"),
                                    "/",
                                    Some("org.freedesktop.DBus.ObjectManager"),
                                    "GetManagedObjects",
                                    &(),
                                ).await {
                                    use std::collections::HashMap;
                                    type ManagedObjects = HashMap<zbus::zvariant::OwnedObjectPath, HashMap<String, HashMap<String, zbus::zvariant::OwnedValue>>>;
                                    if let Ok(objects) = objects_reply.body().deserialize::<ManagedObjects>() {
                                        for (_path, interfaces) in objects {
                                            if let Some(device) = interfaces.get("org.bluez.Device1") {
                                                if let Some(connected) = device.get("Connected") {
                                                    if let Ok(connected) = connected.downcast_ref::<bool>() {
                                                        if connected {
                                                            new_status.bluetooth_connected = true;
                                                            if let Some(name) = device.get("Name") {
                                                                if let Ok(name_str) = name.downcast_ref::<String>() {
                                                                    new_status.bluetooth_devices.push(name_str.clone());
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            let mpris_status = tokio::task::spawn_blocking(move || {
                                let mut media_title = None;
                                let mut media_artist = None;
                                let mut media_art_url = None;
                                let mut media_playing = false;
                                if let Ok(finder) = PlayerFinder::new() {
                                    if let Ok(player) = finder.find_active() {
                                        if let Ok(metadata) = player.get_metadata() {
                                            media_title = metadata.title().map(|s| s.to_string());
                                            media_artist = metadata.artists().map(|a| a.join(", "));
                                            media_art_url = metadata.art_url().map(|u| u.to_string());
                                        }
                                        media_playing = player.get_playback_status().map(|s| matches!(s, mpris::PlaybackStatus::Playing)).unwrap_or(false);
                                    }
                                }
                                (media_title, media_artist, media_art_url, media_playing)
                            }).await.unwrap_or((None, None, None, false));

                            new_status.media_title = mpris_status.0;
                            new_status.media_artist = mpris_status.1;
                            new_status.media_art_url = mpris_status.2;
                            new_status.media_playing = mpris_status.3;

                            if new_status.media_art_url != last_art_url {
                                last_art_url = new_status.media_art_url.clone();
                                last_art_data = None;
                                if let Some(ref url) = last_art_url {
                                    if url.starts_with("file://") {
                                        let path = url.trim_start_matches("file://");
                                        if let Ok(data) = std::fs::read(path) {
                                            last_art_data = Some(Arc::new(data));
                                        }
                                    } else if url.starts_with("http") {
                                        #[cfg(feature = "networking")]
                                        if let Ok(resp) = reqwest::get(url).await {
                                            if let Ok(bytes) = resp.bytes().await {
                                                last_art_data = Some(Arc::new(bytes.to_vec()));
                                            }
                                        }
                                        #[cfg(not(feature = "networking"))]
                                        {
                                            log::debug!("Networking disabled, skipping remote album art: {}", url);
                                        }
                                    }
                                }
                            }
                            new_status.media_art_data = last_art_data.clone();

                            {
                                if let Ok(mut s) = s_clone.lock() {
                                    *s = new_status;
                                }
                            }
                        }
                        Some(cmd) = cmd_rx.recv() => {
                            match cmd {
                                BackendCommand::PowerOff
                                | BackendCommand::Reboot
                                | BackendCommand::Suspend => {
                                    if let Some(ref c) = conn {
                                        // Safety: only PowerOff/Reboot/Suspend reach this
                                        // branch due to the outer match arm.
                                        let method = match cmd {
                                            BackendCommand::PowerOff => "PowerOff",
                                            BackendCommand::Reboot => "Reboot",
                                            BackendCommand::Suspend => "Suspend",
                                            BackendCommand::MediaPlayPause
                                            | BackendCommand::MediaStop
                                            | BackendCommand::MediaNext
                                            | BackendCommand::MediaPrev => {
                                                unreachable!("media command in power branch: {:?}", cmd)
                                            }
                                        };
                                        debug!("Executing system command: {}", method);
                                        let result = tokio::time::timeout(
                                            command_timeout,
                                            c.call_method(
                                                Some("org.freedesktop.login1"),
                                                "/org/freedesktop/login1",
                                                Some("org.freedesktop.login1.Manager"),
                                                method,
                                                &(true),
                                            )
                                        ).await;
                                        if result.is_err() {
                                            error!("System command {} timed out", method);
                                        }
                                    }
                                }
                                BackendCommand::MediaPlayPause
                                | BackendCommand::MediaStop
                                | BackendCommand::MediaNext
                                | BackendCommand::MediaPrev => {
                                    let action = cmd;
                                    // Fire-and-forget: don't block the polling loop on MPRIS.
                                    tokio::task::spawn_blocking(move || {
                                        match PlayerFinder::new() {
                                            Ok(finder) => {
                                                match finder.find_active() {
                                                    Ok(player) => {
                                                        let result = match action {
                                                            BackendCommand::MediaPlayPause => player.play_pause(),
                                                            BackendCommand::MediaStop => player.stop(),
                                                            BackendCommand::MediaNext => player.next(),
                                                            BackendCommand::MediaPrev => player.previous(),
                                                            _ => Ok(()),
                                                        };
                                                        if let Err(e) = result {
                                                            error!("MPRIS command {action:?} failed: {e}");
                                                        }
                                                    }
                                                    Err(e) => error!("No active MPRIS player found: {e}"),
                                                }
                                            }
                                            Err(e) => error!("Failed to create MPRIS PlayerFinder: {e}"),
                                        }
                                    });
                                }
                            }
                        }
                    }
                }
            });
        });

        Self { status, cmd_tx }
    }

    pub fn get_status(&self) -> SystemStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn send_command(&self, cmd: BackendCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn media_play_pause(&self) {
        let _ = self.cmd_tx.send(BackendCommand::MediaPlayPause);
    }

    pub fn media_stop(&self) {
        let _ = self.cmd_tx.send(BackendCommand::MediaStop);
    }

    pub fn media_next(&self) {
        let _ = self.cmd_tx.send(BackendCommand::MediaNext);
    }

    pub fn media_prev(&self) {
        let _ = self.cmd_tx.send(BackendCommand::MediaPrev);
    }
}
