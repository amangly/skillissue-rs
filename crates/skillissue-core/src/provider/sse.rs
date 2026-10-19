//! Incremental server-sent event framing across arbitrary byte chunks.

use super::MAX_EVENT_BYTES;

pub(super) struct SseEvent {
    pub(super) kind: Option<String>,
    pub(super) data: Vec<u8>,
}

#[derive(Default)]
pub(super) struct SseDecoder {
    line: Vec<u8>,
    data: Vec<u8>,
    kind: Option<String>,
    has_data: bool,
    skip_lf: bool,
    seen_line: bool,
}

impl SseDecoder {
    pub(super) fn finish(&self) -> Result<(), String> {
        let unfinished_field = self
            .line
            .split(|byte| *byte == b':')
            .next()
            .unwrap_or_default();
        let partial_event = !unfinished_field.is_empty()
            && (b"data".starts_with(unfinished_field) || b"event".starts_with(unfinished_field));
        if matches!(unfinished_field, b"data" | b"event")
            || partial_event
            || self.has_data
            || self.kind.is_some()
        {
            return Err(
                "The provider stream ended before completion. A stream event was truncated.".into(),
            );
        }
        Ok(())
    }

    pub(super) fn push(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, String> {
        let mut events = Vec::new();
        for &byte in bytes {
            if self.skip_lf {
                self.skip_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }
            if byte == b'\r' || byte == b'\n' {
                self.finish_line(&mut events)?;
                self.skip_lf = byte == b'\r';
            } else {
                if self.line.len() >= MAX_EVENT_BYTES {
                    return Err("The provider stream event exceeded the size limit.".into());
                }
                self.line.push(byte);
            }
        }
        Ok(events)
    }

    fn finish_line(&mut self, events: &mut Vec<SseEvent>) -> Result<(), String> {
        let mut line = std::mem::take(&mut self.line);
        if !self.seen_line {
            self.seen_line = true;
            if line.starts_with(&[0xef, 0xbb, 0xbf]) {
                line.drain(..3);
            }
        }
        if line.is_empty() {
            if self.has_data {
                events.push(SseEvent {
                    kind: self.kind.take(),
                    data: std::mem::take(&mut self.data),
                });
            } else {
                self.kind = None;
            }
            self.has_data = false;
            return Ok(());
        }
        if line[0] == b':' {
            return Ok(());
        }
        let separator = line
            .iter()
            .position(|&byte| byte == b':')
            .unwrap_or(line.len());
        let field = &line[..separator];
        let mut value = if separator < line.len() {
            &line[separator + 1..]
        } else {
            &[]
        };
        if value.first() == Some(&b' ') {
            value = &value[1..];
        }
        if field == b"data" {
            if self
                .data
                .len()
                .saturating_add(value.len())
                .saturating_add(1)
                > MAX_EVENT_BYTES
            {
                return Err("The provider stream event exceeded the size limit.".into());
            }
            if self.has_data {
                self.data.push(b'\n');
            }
            self.data.extend_from_slice(value);
            self.has_data = true;
        } else if field == b"event" {
            self.kind = Some(
                String::from_utf8(value.to_vec())
                    .map_err(|_| "The provider stream contained invalid UTF-8.".to_string())?,
            );
        }
        Ok(())
    }
}
