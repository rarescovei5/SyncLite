use std::net::SocketAddr;

use log::log;
use mdns_sd::{ServiceDaemon, ServiceInfo};

pub fn advertise(port: u16) -> anyhow::Result<()> {
    let mdns = ServiceDaemon::new()?;
    let service_type = "_synclite._tcp.local.";
    let instance_name = "SyncLite";
    let host_name = format!("{}.local.", instance_name);
    let my_ip = local_ip_address::local_ip().unwrap();
    let service_info = ServiceInfo::new(
        service_type,
        &instance_name,
        &host_name,
        my_ip.to_string().as_str(),
        port,
        None,
    )?;

    mdns.register(service_info)?;

    Ok(())
}

pub async fn browse(port: u16) -> anyhow::Result<SocketAddr> {
    // Discover mDNS service
    let mdns = ServiceDaemon::new()?;
    let service_type = "_synclite._tcp.local.";
    let receiver = mdns.browse(service_type)?;

    log!(info, "Browsing for SyncLite servers...");

    let addr = loop {
        match receiver.recv() {
            Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                log!(info, "Resolved service: {}", info.get_fullname());
                if let Some(ip) = info.get_addresses().iter().next() {
                    let ip_addr: std::net::IpAddr = ip.to_string().parse().unwrap();
                    let host_port = info.get_port();

                    if host_port != port {
                        continue;
                    }

                    let addr = SocketAddr::new(ip_addr, port);
                    log!(info, "Found server at: {}", addr);
                    break addr;
                }
            }
            _ => {}
        }
    };

    Ok(addr)
}
