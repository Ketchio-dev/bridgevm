use std::time::{Duration, Instant};

use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_virt::VirtPlatform;

const TEST_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_TEST";
const CMDS_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CMDS";
const TIMEOUT_MS_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS";
const DEFAULT_CMDS: &str = "whoami|ver|ipconfig";
const DEFAULT_TIMEOUT_MS: u64 = 180_000;

pub struct AgentConsoleHarness {
    start: Instant,
    timeout: Duration,
    framer: LineFramer,
    state: AgentConsoleState,
    commands: Vec<String>,
    last_ping: Option<Instant>,
}

const PING_INTERVAL: Duration = Duration::from_secs(3);

enum AgentConsoleState {
    WaitingReady,
    WaitingPong,
    WaitingOut { index: usize },
    Done,
    TimedOut,
}

impl AgentConsoleHarness {
    pub fn from_env(start: Instant) -> Option<Self> {
        env_flag(TEST_ENV).then(|| Self {
            start,
            timeout: Duration::from_millis(env_u64(TIMEOUT_MS_ENV, DEFAULT_TIMEOUT_MS)),
            framer: LineFramer::new(),
            state: AgentConsoleState::WaitingReady,
            commands: agent_commands_from_env(),
            last_ping: None,
        })
    }

    pub fn tick(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) {
        if matches!(
            self.state,
            AgentConsoleState::Done | AgentConsoleState::TimedOut
        ) {
            return;
        }

        let inbound = platform.virtio_console_agent_take_inbound();
        let lines = self.framer.push(&inbound);
        for line in lines {
            self.handle_line(&line, platform, mem, now);
        }

        // Proactively PING while waiting to connect. The agent may have sent
        // its one-shot READY before the guest driver's host-open latched (that
        // write is silently dropped), so waiting passively for READY can
        // deadlock. A PING the agent echoes as PONG breaks that: any inbound
        // line (READY or PONG) advances us to the command phase.
        if matches!(self.state, AgentConsoleState::WaitingReady) {
            let due = self
                .last_ping
                .is_none_or(|t| now.duration_since(t) >= PING_INTERVAL);
            if due {
                platform.virtio_console_agent_send(b"PING\n", mem);
                self.last_ping = Some(now);
            }
        }

        if matches!(
            self.state,
            AgentConsoleState::WaitingReady | AgentConsoleState::WaitingPong
        ) && now.duration_since(self.start) >= self.timeout
        {
            println!("BVAGENT TIMEOUT waiting for READY");
            self.state = AgentConsoleState::TimedOut;
        }
    }

    fn handle_line(
        &mut self,
        line: &str,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) {
        match self.state {
            AgentConsoleState::WaitingReady => {
                // Connect on READY (agent hello) OR PONG (reply to a proactive
                // PING when READY was lost). Either proves the channel is live.
                if let Some(hostname) = line.strip_prefix("READY ") {
                    println!(
                        "BVAGENT READY host={} t={}",
                        hostname,
                        now.duration_since(self.start).as_millis()
                    );
                    platform.virtio_console_agent_send(b"PING\n", mem);
                    self.state = AgentConsoleState::WaitingPong;
                } else if line == "PONG" {
                    println!(
                        "BVAGENT PONG (proactive) t={}",
                        now.duration_since(self.start).as_millis()
                    );
                    self.send_next_command_or_done(0, platform, mem);
                }
            }
            AgentConsoleState::WaitingPong => {
                if line != "PONG" {
                    return;
                }
                println!("BVAGENT PONG");
                self.send_next_command_or_done(0, platform, mem);
            }
            AgentConsoleState::WaitingOut { index } => {
                let Some((exit_code, output)) = parse_out_line(line) else {
                    return;
                };
                let command = &self.commands[index];
                match base64_decode(output) {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        println!(
                            "BVAGENT CMD {command} exit={exit_code}\n{text}\nBVAGENT END {command}"
                        );
                    }
                    Err(error) => {
                        println!(
                            "BVAGENT CMD {command} exit={exit_code}\n<base64 decode error: {error:?}>\nBVAGENT END {command}"
                        );
                    }
                }
                self.send_next_command_or_done(index + 1, platform, mem);
            }
            AgentConsoleState::Done | AgentConsoleState::TimedOut => {}
        }
    }

    fn send_next_command_or_done(
        &mut self,
        index: usize,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
    ) {
        let Some(command) = self.commands.get(index) else {
            println!("BVAGENT DONE");
            self.state = AgentConsoleState::Done;
            return;
        };
        let encoded = base64_encode(command.as_bytes());
        let line = format!("RUN {encoded}\n");
        platform.virtio_console_agent_send(line.as_bytes(), mem);
        self.state = AgentConsoleState::WaitingOut { index };
    }
}

#[derive(Default)]
struct LineFramer {
    pending: Vec<u8>,
}

impl LineFramer {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(bytes);
        let mut lines = Vec::new();
        while let Some(newline) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut raw = self.pending.drain(..=newline).collect::<Vec<_>>();
            raw.pop();
            if raw.last() == Some(&b'\r') {
                raw.pop();
            }
            lines.push(String::from_utf8_lossy(&raw).into_owned());
        }
        lines
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Base64DecodeError {
    InvalidByte,
    InvalidLength,
    InvalidPadding,
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() >= 2 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() == 3 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn base64_decode(text: &str) -> Result<Vec<u8>, Base64DecodeError> {
    let bytes = text.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(Base64DecodeError::InvalidLength);
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut saw_padding = false;
    for chunk in bytes.chunks(4) {
        let mut vals = [0u8; 4];
        let mut padding = 0usize;
        for (i, byte) in chunk.iter().copied().enumerate() {
            match byte {
                b'A'..=b'Z' if !saw_padding => vals[i] = byte - b'A',
                b'a'..=b'z' if !saw_padding => vals[i] = byte - b'a' + 26,
                b'0'..=b'9' if !saw_padding => vals[i] = byte - b'0' + 52,
                b'+' if !saw_padding => vals[i] = 62,
                b'/' if !saw_padding => vals[i] = 63,
                b'=' => {
                    saw_padding = true;
                    padding += 1;
                    if i < 2 {
                        return Err(Base64DecodeError::InvalidPadding);
                    }
                }
                _ => return Err(Base64DecodeError::InvalidByte),
            }
        }
        if padding > 2 {
            return Err(Base64DecodeError::InvalidPadding);
        }
        if padding > 0 && chunk[3] != b'=' {
            return Err(Base64DecodeError::InvalidPadding);
        }
        out.push((vals[0] << 2) | (vals[1] >> 4));
        if padding < 2 {
            out.push((vals[1] << 4) | (vals[2] >> 2));
        }
        if padding == 0 {
            out.push((vals[2] << 6) | vals[3]);
        }
    }
    Ok(out)
}

fn parse_out_line(line: &str) -> Option<(i32, &str)> {
    let rest = line.strip_prefix("OUT ")?;
    let (exit_code, output) = rest.split_once(' ')?;
    Some((exit_code.parse().ok()?, output))
}

fn agent_commands_from_env() -> Vec<String> {
    let value = std::env::var(CMDS_ENV).unwrap_or_else(|_| DEFAULT_CMDS.to_string());
    value
        .split('|')
        .map(str::trim)
        .filter(|cmd| !cmd.is_empty())
        .map(str::to_string)
        .collect()
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

fn env_flag(name: &str) -> bool {
    let Ok(value) = std::env::var(name) else {
        return false;
    };
    let value = value.trim();
    value == "1"
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_known_vectors_encode_and_decode() {
        let vectors = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];

        for (plain, encoded) in vectors {
            assert_eq!(base64_encode(plain.as_bytes()), encoded);
            assert_eq!(base64_decode(encoded).unwrap(), plain.as_bytes());
        }
    }

    #[test]
    fn base64_round_trips_binary_payloads() {
        let payloads = [
            Vec::new(),
            vec![0],
            vec![0, 1],
            vec![0, 1, 2],
            (0u8..=255).collect::<Vec<_>>(),
        ];

        for payload in payloads {
            assert_eq!(base64_decode(&base64_encode(&payload)).unwrap(), payload);
        }
    }

    #[test]
    fn line_framer_handles_partial_crlf_and_multiple_lines() {
        let mut framer = LineFramer::new();

        assert!(framer.push(b"REA").is_empty());
        assert_eq!(
            framer.push(b"DY host\r\nPONG\nOUT"),
            vec!["READY host", "PONG"]
        );
        assert!(framer.push(b" 0 Zm9v").is_empty());
        assert_eq!(framer.push(b"\n").as_slice(), ["OUT 0 Zm9v"]);
    }
}
