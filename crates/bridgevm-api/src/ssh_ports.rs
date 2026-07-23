//! Split out of lib.rs by responsibility.

use crate::*;

pub fn ssh_plan(store: &VmStore, name: &str, user: Option<&str>) -> Result<SshPlanRecord, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let user = user.unwrap_or("user").to_string();
    if user.trim().is_empty() {
        return Err("ssh user must not be empty".to_string());
    }

    if manifest.mode == VmMode::Compatibility {
        if let Some(plan) = ssh_plan_from_forward(name, user.clone(), &manifest.network.forwards) {
            return Ok(plan);
        }
    }

    if let Some(runtime) = store
        .guest_tools_runtime_metadata(name)
        .map_err(|error| error.to_string())?
    {
        if runtime.connected {
            if let Some(address) = runtime
                .guest_ip_addresses
                .iter()
                .map(|address| address.address.trim())
                .find(|address| valid_guest_ip(address))
            {
                return Ok(ssh_plan_record(
                    name,
                    user,
                    address.to_string(),
                    22,
                    SshPlanSource::GuestToolsIp,
                ));
            }
        }
    }

    if manifest.mode != VmMode::Compatibility {
        if let Some(plan) = ssh_plan_from_forward(name, user, &manifest.network.forwards) {
            return Ok(plan);
        }
    }

    Err(format!(
        "no SSH target available for {name}; report a reachable guest IP through guest tools or add a port forward such as 2222:22"
    ))
}

pub fn open_port_plan(
    store: &VmStore,
    name: &str,
    guest_port: u16,
    scheme: Option<&str>,
) -> Result<OpenPortPlanRecord, String> {
    if guest_port == 0 {
        return Err("guest port must be between 1 and 65535".to_string());
    }
    let scheme = normalized_url_scheme(scheme.unwrap_or("http"))?;
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let forward = manifest
        .network
        .forwards
        .iter()
        .filter(|forward| forward.guest == guest_port)
        .min_by_key(|forward| forward.host)
        .ok_or_else(|| {
            format!(
                "no host port is forwarded to guest port {guest_port}; add one with: bridgevm port add {name} <host>:{guest_port}"
            )
        })?;
    let host = "127.0.0.1".to_string();
    let url = format!("{scheme}://{host}:{}", forward.host);
    Ok(OpenPortPlanRecord {
        vm: name.to_string(),
        scheme,
        host,
        guest_port,
        host_port: forward.host,
        command: vec!["open".to_string(), url.clone()],
        url,
    })
}

pub(crate) fn normalized_url_scheme(scheme: &str) -> Result<String, String> {
    let scheme = scheme.trim().to_ascii_lowercase();
    if scheme.is_empty() {
        return Err("URL scheme must not be empty".to_string());
    }
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return Err("URL scheme must not be empty".to_string());
    };
    if !first.is_ascii_alphabetic() {
        return Err("URL scheme must start with an ASCII letter".to_string());
    }
    if !chars.all(|char| char.is_ascii_alphanumeric() || matches!(char, '+' | '-' | '.')) {
        return Err(
            "URL scheme may only contain ASCII letters, digits, '+', '-', or '.'".to_string(),
        );
    }
    Ok(scheme)
}

pub(crate) fn ssh_plan_from_forward(
    name: &str,
    user: String,
    forwards: &[PortForward],
) -> Option<SshPlanRecord> {
    forwards
        .iter()
        .filter(|forward| forward.guest == 22)
        .min_by_key(|forward| forward.host)
        .map(|forward| {
            ssh_plan_record(
                name,
                user,
                "127.0.0.1".to_string(),
                forward.host,
                SshPlanSource::PortForward,
            )
        })
}

pub(crate) fn valid_guest_ip(address: &str) -> bool {
    match address.parse::<IpAddr>() {
        Ok(IpAddr::V4(address)) => {
            !address.is_unspecified() && !address.is_loopback() && !address.is_link_local()
        }
        Ok(IpAddr::V6(address)) => {
            let first_segment = address.segments()[0];
            !address.is_unspecified() && !address.is_loopback() && first_segment & 0xffc0 != 0xfe80
        }
        Err(_) => false,
    }
}

pub(crate) fn ssh_plan_record(
    name: &str,
    user: String,
    host: String,
    port: u16,
    source: SshPlanSource,
) -> SshPlanRecord {
    let mut command = vec!["ssh".to_string()];
    if port != 22 {
        command.extend(["-p".to_string(), port.to_string()]);
    }
    command.push(format!("{user}@{host}"));
    SshPlanRecord {
        vm: name.to_string(),
        user,
        host,
        port,
        source,
        command,
    }
}
