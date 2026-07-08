use std::{
    fmt,
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
};

pub(crate) struct VirtioGpuTraceRecorder {
    sink: Option<TraceSink>,
    seq: u64,
}

enum TraceSink {
    Stdout,
    File(File),
}

impl fmt::Debug for VirtioGpuTraceRecorder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VirtioGpuTraceRecorder")
            .field("enabled", &self.sink.is_some())
            .field("seq", &self.seq)
            .finish()
    }
}

impl Default for VirtioGpuTraceRecorder {
    fn default() -> Self {
        Self::from_env()
    }
}

impl VirtioGpuTraceRecorder {
    pub(crate) fn from_env() -> Self {
        if let Ok(path) = std::env::var("BRIDGEVM_VIRTIO_GPU_TRACE_JSONL") {
            if !path.trim().is_empty() {
                return Self::file(path);
            }
        }

        match std::env::var("BRIDGEVM_VIRTIO_GPU_TRACE") {
            Ok(value) if trace_truthy(&value) => Self {
                sink: Some(TraceSink::Stdout),
                seq: 0,
            },
            Ok(value) if !value.trim().is_empty() && !trace_falsey(&value) => Self::file(value),
            _ => Self::disabled(),
        }
    }

    pub(crate) fn disabled() -> Self {
        Self { sink: None, seq: 0 }
    }

    fn file(path: impl AsRef<Path>) -> Self {
        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.as_ref())
        {
            Ok(file) => Self {
                sink: Some(TraceSink::File(file)),
                seq: 0,
            },
            Err(error) => {
                eprintln!(
                    "virtio-gpu trace: disabling recorder; failed to open {}: {error}",
                    path.as_ref().display()
                );
                Self::disabled()
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn test_file(path: impl AsRef<Path>) -> Self {
        Self::file(path)
    }

    pub(crate) fn enabled(&self) -> bool {
        self.sink.is_some()
    }

    pub(crate) fn record(&mut self, event: &str, fields: impl AsRef<str>) {
        if self.sink.is_none() {
            return;
        }
        self.seq = self.seq.saturating_add(1);
        let line = format!(
            "{{\"seq\":{},\"event\":{}{} }}\n",
            self.seq,
            json_string(event),
            fields.as_ref()
        );
        match self.sink.as_mut().unwrap() {
            TraceSink::Stdout => {
                print!("{line}");
            }
            TraceSink::File(file) => {
                let _ = file.write_all(line.as_bytes());
                let _ = file.flush();
            }
        }
    }
}

pub(crate) fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ch if ch.is_control() => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn trace_truthy(value: &str) -> bool {
    matches!(
        value.trim(),
        "1" | "true" | "TRUE" | "yes" | "YES" | "stdout" | "STDOUT"
    )
}

fn trace_falsey(value: &str) -> bool {
    matches!(value.trim(), "0" | "false" | "FALSE" | "no" | "NO")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_string_escapes_control_characters() {
        assert_eq!(
            json_string("gpu \"trace\"\\line\n"),
            "\"gpu \\\"trace\\\"\\\\line\\n\""
        );
    }

    #[test]
    fn file_recorder_writes_jsonl_events() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-virtio-gpu-trace-{}-{}.jsonl",
            std::process::id(),
            unique_suffix()
        ));
        let mut recorder = VirtioGpuTraceRecorder::test_file(&path);
        recorder.record("command", ",\"typ\":256,\"name\":\"GET_DISPLAY_INFO\"");
        drop(recorder);

        let contents = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(path);
        assert!(contents.contains("\"seq\":1"));
        assert!(contents.contains("\"event\":\"command\""));
        assert!(contents.contains("\"typ\":256"));
        assert!(contents.ends_with('\n'));
    }

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }
}
