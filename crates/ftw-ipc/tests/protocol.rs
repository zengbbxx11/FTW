use ftw_ipc::{read_frame, write_frame, Operation, Request, PROTOCOL_VERSION};
use std::io;
use tokio::io::{duplex, AsyncWriteExt};

#[tokio::test]
async fn health_request_round_trips() {
    let (mut writer, mut reader) = duplex(256);
    let request = Request {
        version: PROTOCOL_VERSION,
        operation: Operation::Health,
    };
    write_frame(&mut writer, &request).await.unwrap();
    let incoming = read_frame::<Request>(&mut reader).await.unwrap();
    assert_eq!(incoming.version, PROTOCOL_VERSION);
    assert!(matches!(incoming.operation, Operation::Health));
}

#[tokio::test]
async fn oversized_frame_is_rejected_before_reading_a_body() {
    let (mut writer, mut reader) = duplex(16);
    writer.write_u32_le(u32::MAX).await.unwrap();
    let error = read_frame::<Request>(&mut reader).await.unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}
