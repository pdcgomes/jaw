//! Generates Rust types for the engine link (proto/engine/) and for the WhatsApp protobufs jawd
//! reads (proto/whatsapp/, copied from whatsmeow by scripts/sync-protos.py), with protox, so
//! building needs no protoc. The packages these import come along on their own.

use std::path::PathBuf;

const WHATSAPP: &[&str] = &[
    "waE2E/WAWebProtobufsE2E.proto",
    "waHistorySync/WAWebProtobufsHistorySync.proto",
    "waWeb/WAWebProtobufsWeb.proto",
    "waCommon/WACommon.proto",
    "waMmsRetry/WAMmsRetry.proto",
    "waSyncAction/WAWebProtobufSyncAction.proto",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../proto");
    let engine = root.join("engine");
    let whatsapp = root.join("whatsapp");
    let mut files = vec![engine.join("engine.proto")];
    files.extend(WHATSAPP.iter().map(|f| whatsapp.join(f)));
    println!("cargo:rerun-if-changed={}", root.display());
    let descriptors = protox::compile(&files, [&engine, &whatsapp])?;
    prost_build::Config::new().include_file("protos.rs").compile_fds(descriptors)?;
    Ok(())
}
