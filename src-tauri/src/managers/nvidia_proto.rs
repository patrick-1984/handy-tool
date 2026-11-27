//! The small subset of Riva's protobuf and unary gRPC framing used by Recognize.
//! No generated code or protobuf runtime. All lengths and wire types are checked.
use std::fmt;

#[derive(Debug)]
pub struct ProtocolError(pub String);
impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ProtocolError {}
type Result<T> = std::result::Result<T, ProtocolError>;
fn invalid(message: &str) -> ProtocolError {
    ProtocolError(message.to_string())
}

fn varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 128 {
        out.push(value as u8 | 128);
        value >>= 7;
    }
    out.push(value as u8);
}
fn bytes(field: u8, value: &[u8], out: &mut Vec<u8>) {
    varint(u64::from(field) << 3 | 2, out);
    varint(value.len() as u64, out);
    out.extend_from_slice(value);
}

/// PCM16 little endian, mono, 16 kHz, with punctuation and one alternative.
pub fn recognize_request(pcm: &[u8], language: &str) -> Vec<u8> {
    let mut config = vec![0x08, 1, 0x10]; // encoding = LINEAR_PCM, sample_rate_hertz
    varint(16_000, &mut config);
    bytes(3, language.as_bytes(), &mut config);
    config.extend_from_slice(&[0x20, 1, 0x38, 1, 0x58, 1]);
    let mut request = Vec::with_capacity(pcm.len() + config.len() + 16);
    bytes(1, &config, &mut request);
    bytes(2, pcm, &mut request);
    let mut frame = Vec::with_capacity(request.len() + 5);
    frame.push(0); // no compression
    frame.extend_from_slice(&(request.len() as u32).to_be_bytes());
    frame.extend(request);
    frame
}

fn read_varint(input: &mut &[u8]) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let (&byte, rest) = input
            .split_first()
            .ok_or_else(|| invalid("Truncated protobuf varint"))?;
        *input = rest;
        if shift == 63 && byte > 1 {
            return Err(invalid("Protobuf varint overflow"));
        }
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    Err(invalid("Protobuf varint overflow"))
}
fn take<'a>(input: &mut &'a [u8], len: usize) -> Result<&'a [u8]> {
    if len > input.len() {
        return Err(invalid("Truncated protobuf field"));
    }
    let (value, rest) = input.split_at(len);
    *input = rest;
    Ok(value)
}

/// Iterate fields, skipping unknown scalar fields (confidence, timestamps, etc.).
fn fields(mut input: &[u8], mut visit: impl FnMut(u64, &[u8]) -> Result<()>) -> Result<()> {
    while !input.is_empty() {
        let tag = read_varint(&mut input)?;
        if tag >> 3 == 0 {
            return Err(invalid("Invalid protobuf field number"));
        }
        match tag & 7 {
            0 => {
                read_varint(&mut input)?;
            }
            1 => {
                take(&mut input, 8)?;
            }
            2 => {
                let len = usize::try_from(read_varint(&mut input)?)
                    .map_err(|_| invalid("Protobuf length overflow"))?;
                visit(tag >> 3, take(&mut input, len)?)?;
            }
            5 => {
                take(&mut input, 4)?;
            }
            _ => return Err(invalid("Unsupported protobuf wire type")),
        }
    }
    Ok(())
}

/// One unary RecognizeResponse; choose the first alternative of every result.
pub fn recognize_response(frame: &[u8]) -> Result<String> {
    if frame.len() < 5 {
        return Err(invalid("Truncated gRPC frame"));
    }
    if frame[0] != 0 {
        return Err(invalid("Compressed gRPC responses are unsupported"));
    }
    let len = u32::from_be_bytes(frame[1..5].try_into().unwrap()) as usize;
    if frame.len() - 5 != len {
        return Err(invalid("Invalid unary gRPC frame length"));
    }
    let mut texts = Vec::new();
    fields(&frame[5..], |number, result| {
        if number != 1 {
            return Ok(());
        }
        let mut first = true;
        fields(result, |number, alternative| {
            if number != 1 || !first {
                return Ok(());
            }
            first = false;
            fields(alternative, |number, value| {
                if number == 1 {
                    let text = std::str::from_utf8(value)
                        .map_err(|_| invalid("Invalid UTF-8 in Riva transcript"))?
                        .trim();
                    if !text.is_empty() {
                        texts.push(text.to_string());
                    }
                }
                Ok(())
            })
        })
    })?;
    Ok(texts.join(" "))
}

/// gRPC percent encoding uses %HH; '+' is literal, unlike form encoding.
fn decode_message(message: &str) -> String {
    let mut output = Vec::new();
    let mut input = message.as_bytes();
    while !input.is_empty() {
        if input.len() >= 3 && input[0] == b'%' {
            if let (Some(a), Some(b)) = (
                (input[1] as char).to_digit(16),
                (input[2] as char).to_digit(16),
            ) {
                output.push((a * 16 + b) as u8);
                input = &input[3..];
                continue;
            }
        }
        output.push(input[0]);
        input = &input[1..];
    }
    String::from_utf8_lossy(&output).into_owned()
}

pub fn grpc_status(status: Option<&str>, message: Option<&str>) -> Result<()> {
    match status {
        Some("0") => Ok(()),
        Some(code) => {
            let detail = decode_message(message.unwrap_or("No details from NVIDIA"));
            let hint = match code {
                "7" | "16" => " API key refused; check your NVIDIA API key and function access.",
                "4" => " The NVIDIA request timed out.",
                _ => "",
            };
            Err(ProtocolError(format!(
                "NVIDIA gRPC status {code}: {detail}.{hint}"
            )))
        }
        None => Err(invalid(
            "NVIDIA response is missing grpc-status (HTTP/2 trailers required)",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0];
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(payload);
        out
    }
    #[test]
    fn request_matches_riva_wire_bytes() {
        assert_eq!(
            recognize_request(&[0, 0, 255, 127], "en-US"),
            vec![
                0, 0, 0, 0, 26, 10, 18, 8, 1, 16, 128, 125, 26, 5, b'e', b'n', b'-', b'U', b'S',
                32, 1, 56, 1, 88, 1, 18, 4, 0, 0, 255, 127,
            ]
        );
    }
    #[test]
    fn decodes_captured_response() {
        // Captured from NVIDIA hosted Whisper using the authorized public sample.
        let response = [
            0x00, 0x00, 0x00, 0x00, 0x5f, 0x0a, 0x34, 0x0a, 0x2b, 0x0a, 0x25, 0x57, 0x68, 0x61,
            0x74, 0x20, 0x69, 0x73, 0x20, 0x6e, 0x61, 0x74, 0x75, 0x72, 0x61, 0x6c, 0x20, 0x6c,
            0x61, 0x6e, 0x67, 0x75, 0x61, 0x67, 0x65, 0x20, 0x70, 0x72, 0x6f, 0x63, 0x65, 0x73,
            0x73, 0x69, 0x6e, 0x67, 0x3f, 0x20, 0x22, 0x02, 0x65, 0x6e, 0x10, 0x01, 0x1d, 0x2f,
            0xdd, 0x84, 0x40, 0xa2, 0x06, 0x26, 0x0a, 0x24, 0x38, 0x64, 0x38, 0x65, 0x30, 0x61,
            0x33, 0x39, 0x2d, 0x61, 0x62, 0x38, 0x65, 0x2d, 0x34, 0x36, 0x32, 0x37, 0x2d, 0x38,
            0x36, 0x33, 0x31, 0x2d, 0x66, 0x35, 0x63, 0x32, 0x33, 0x64, 0x37, 0x62, 0x64, 0x35,
            0x37, 0x66,
        ];
        assert_eq!(
            recognize_response(&response).unwrap(),
            "What is natural language processing?"
        );
    }
    #[test]
    fn empty_results_are_successful() {
        assert_eq!(recognize_response(&frame(&[])).unwrap(), "");
        assert_eq!(recognize_response(&frame(&[10, 0])).unwrap(), "");
    }
    #[test]
    fn first_alternative_of_multiple_results() {
        let mut result = vec![];
        bytes(1, b"\x0a\x03one", &mut result);
        bytes(1, b"\x0a\x03two", &mut result);
        let mut response = vec![];
        bytes(1, &result, &mut response);
        bytes(1, b"\x0a\x07\x0a\x05three", &mut response);
        assert_eq!(recognize_response(&frame(&response)).unwrap(), "one three");
    }
    #[test]
    fn grpc_errors_and_missing_status() {
        grpc_status(Some("0"), None).unwrap();
        let error = grpc_status(Some("16"), Some("key%20refused%3A%20a+b"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("key refused: a+b"));
        assert!(error.contains("API key refused"));
        assert!(
            grpc_status(Some("4"), None)
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
        assert!(grpc_status(None, None).is_err());
    }
    #[test]
    fn rejects_truncated_compressed_and_extra_frames() {
        for data in [
            vec![],
            vec![0, 0, 0, 0, 5],
            vec![1, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0, 0],
            frame(&[10, 255]),
            frame(&[0]),
            frame(&[10, 1, 255]),
        ] {
            assert!(recognize_response(&data).is_err(), "{data:?}");
        }
    }
    #[test]
    fn rejects_overflow_and_skips_unknown_fields() {
        assert!(recognize_response(&frame(&[128; 11])).is_err());
        assert!(
            recognize_response(&frame(&[
                10, 255, 255, 255, 255, 255, 255, 255, 255, 255, 2
            ]))
            .is_err()
        );
        assert_eq!(
            recognize_response(&frame(&[
                16, 1, 29, 0, 0, 0, 0, 33, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0
            ]))
            .unwrap(),
            ""
        );
    }
}
