//! Service Manager
//!
//! Systemd/RC-like service management for CantayaOS.
//! Tracks system daemons with start/stop/enable/disable capabilities.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use spin::Mutex;

extern crate alloc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ServiceState {
    Running,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ServiceType {
    System,
    Network,
    User,
}

#[derive(Clone, Debug)]
pub struct Service {
    pub name: String,
    pub description: String,
    pub state: ServiceState,
    pub enabled: bool,
    pub service_type: ServiceType,
    pub pid: u32,
    pub started_at: u64,     // ms since boot
    pub restart_count: u32,
    pub port: Option<u16>,   // listening port if any
}

struct ServiceManager {
    services: BTreeMap<String, Service>,
}

impl ServiceManager {
    const fn new() -> Self {
        ServiceManager {
            services: BTreeMap::new(),
        }
    }
}

static SERVICES: Mutex<ServiceManager> = Mutex::new(ServiceManager::new());

/// Initialize the service manager with default system services
pub fn init() {
    let mut mgr = SERVICES.lock();
    mgr.services = BTreeMap::new();

    // Core system services
    let defaults = [
        ("syslogd", "System Log Daemon", ServiceType::System, true, Some(514u16)),
        ("crond", "Cron Scheduler", ServiceType::System, true, None),
        ("sshd", "Secure Shell Daemon", ServiceType::Network, true, Some(22)),
        ("dhcpcd", "DHCP Client Daemon", ServiceType::Network, true, Some(68)),
        ("ntpd", "Network Time Protocol", ServiceType::Network, true, Some(123)),
        ("dbus", "D-Bus Message Bus", ServiceType::System, true, None),
        ("udevd", "Device Manager", ServiceType::System, true, None),
        ("getty", "Virtual Console Login", ServiceType::User, true, None),
        ("httpd", "HTTP Server", ServiceType::Network, false, Some(80)),
        ("cupsd", "Print Service", ServiceType::User, false, Some(631)),
    ];

    let mut pid = 100u32;
    for (name, desc, stype, enabled, port) in &defaults {
        let state = if *enabled { ServiceState::Running } else { ServiceState::Stopped };
        let started = if *enabled { 500 + pid as u64 * 3 } else { 0 };
        mgr.services.insert(String::from(*name), Service {
            name: String::from(*name),
            description: String::from(*desc),
            state,
            enabled: *enabled,
            service_type: *stype,
            pid: if *enabled { pid } else { 0 },
            started_at: started,
            restart_count: 0,
            port: *port,
        });
        pid += 1;
    }
}

/// List all services
pub fn list_services() -> Vec<Service> {
    let mgr = SERVICES.lock();
    mgr.services.values().cloned().collect()
}

/// Get a specific service
pub fn get_service(name: &str) -> Option<Service> {
    let mgr = SERVICES.lock();
    mgr.services.get(name).cloned()
}

/// Start a service
pub fn start_service(name: &str) -> Result<(), &'static str> {
    let mut mgr = SERVICES.lock();
    if let Some(svc) = mgr.services.get_mut(name) {
        if svc.state == ServiceState::Running {
            return Err("already running");
        }
        svc.state = ServiceState::Running;
        svc.pid = 200 + (name.len() as u32 * 7) % 100;
        svc.started_at = crate::hal::timer::uptime_ms();
        svc.restart_count += 1;
        Ok(())
    } else {
        Err("service not found")
    }
}

/// Stop a service
pub fn stop_service(name: &str) -> Result<(), &'static str> {
    let mut mgr = SERVICES.lock();
    if let Some(svc) = mgr.services.get_mut(name) {
        if svc.state == ServiceState::Stopped {
            return Err("already stopped");
        }
        svc.state = ServiceState::Stopped;
        svc.pid = 0;
        Ok(())
    } else {
        Err("service not found")
    }
}

/// Restart a service
pub fn restart_service(name: &str) -> Result<(), &'static str> {
    let mut mgr = SERVICES.lock();
    if let Some(svc) = mgr.services.get_mut(name) {
        svc.state = ServiceState::Running;
        svc.pid = 200 + (name.len() as u32 * 7) % 100;
        svc.started_at = crate::hal::timer::uptime_ms();
        svc.restart_count += 1;
        Ok(())
    } else {
        Err("service not found")
    }
}

/// Enable a service (auto-start at boot)
pub fn enable_service(name: &str) -> Result<(), &'static str> {
    let mut mgr = SERVICES.lock();
    if let Some(svc) = mgr.services.get_mut(name) {
        svc.enabled = true;
        Ok(())
    } else {
        Err("service not found")
    }
}

/// Disable a service
pub fn disable_service(name: &str) -> Result<(), &'static str> {
    let mut mgr = SERVICES.lock();
    if let Some(svc) = mgr.services.get_mut(name) {
        svc.enabled = false;
        Ok(())
    } else {
        Err("service not found")
    }
}

/// Count running services
pub fn running_count() -> usize {
    let mgr = SERVICES.lock();
    mgr.services.values().filter(|s| s.state == ServiceState::Running).count()
}

/// Generate content for /proc/services
pub fn generate_services_content() -> String {
    let mgr = SERVICES.lock();
    let mut out = String::from("Name            State     Enabled  PID    Type\n");
    for svc in mgr.services.values() {
        let state = match svc.state {
            ServiceState::Running => "running",
            ServiceState::Stopped => "stopped",
            ServiceState::Failed  => "failed",
        };
        let enabled = if svc.enabled { "yes" } else { "no" };
        let stype = match svc.service_type {
            ServiceType::System  => "system",
            ServiceType::Network => "network",
            ServiceType::User    => "user",
        };
        out.push_str(&format!("{:<16}{:<10}{:<9}{:<7}{}\n",
            svc.name, state, enabled, svc.pid, stype));
    }
    out
}
