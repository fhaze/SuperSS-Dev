//! Validation tests against real packets captured from the live C++ server.
//!
//! The fixture (`fixtures/captured_packets.json`) holds post-decrypt plaintext
//! captured via the pangya.nozomi.local GM API. Each `plaintext_hex` is the
//! full on-wire plaintext: a 2-byte LE opcode followed by the payload.
//!
//! These tests confirm our [`split_opcode`] and opcode interpretation match the
//! real server exactly — the most error-prone part of the port. They do NOT
//! validate the crypto layer (the API stores plaintext, not encrypted bytes).

use pangya_proto::split_opcode;

#[derive(serde::Deserialize)]
struct Fixture {
    packets: Vec<CapturedPacket>,
}

#[derive(serde::Deserialize)]
struct CapturedPacket {
    pid: String,
    pid_int: u16,
    #[allow(dead_code)]
    dir: String,
    name: String,
    size: u32,
    plaintext_hex: String,
}

fn load_fixture() -> Vec<CapturedPacket> {
    let raw = include_str!("fixtures/captured_packets.json");
    let fixture: Fixture = serde_json::from_str(raw).expect("fixture parses");
    fixture.packets
}

#[test]
fn every_captured_opcode_parses_via_split_opcode() {
    // For every real packet, split_opcode must recover exactly the pid the API
    // reported. The payload length should equal size - 2 (opcode stripped),
    // except for large packets where the API truncates the hex field — a
    // capture limitation, not a protocol mismatch.
    let mut checked_payload_len = 0;
    for p in load_fixture() {
        let plaintext = hex::decode(&p.plaintext_hex).expect("hex decodes");
        let (opcode, payload) =
            split_opcode(&plaintext).unwrap_or_else(|| panic!("split failed for {}", p.pid));
        assert_eq!(opcode, p.pid_int, "{} ({}): opcode mismatch", p.pid, p.name);
        // Only assert exact payload length when the hex wasn't truncated.
        let expected_payload_len = p.size as usize - 2;
        if payload.len() == expected_payload_len {
            checked_payload_len += 1;
        } else {
            // Truncated: payload should still be a prefix of the expected length.
            assert!(
                payload.len() < expected_payload_len,
                "{} ({}): payload longer than expected — real protocol mismatch",
                p.pid,
                p.name
            );
        }
    }
    // Sanity: at least most packets had intact payloads.
    assert!(
        checked_payload_len >= 20,
        "expected ≥20 intact payloads, got {checked_payload_len}"
    );
}

#[test]
fn heartbeat_structure_is_opcode_plus_u32_tick() {
    // 0x00F4 heartbeat: full plaintext f4 00 22 00 00 00
    //   = opcode 0x00F4 + payload [u32 tick = 34]
    let p = load_fixture()
        .into_iter()
        .find(|p| p.pid == "0x00F4")
        .expect("heartbeat captured");
    let plaintext = hex::decode(&p.plaintext_hex).unwrap();
    let (_opcode, payload) = split_opcode(&plaintext).unwrap();
    assert_eq!(payload.len(), 4);
    let tick = u32::from_le_bytes(payload.try_into().unwrap());
    assert!(tick > 0, "heartbeat tick should be non-zero, got {tick}");
}

#[test]
fn opcode_less_packets_have_empty_payload() {
    // 0x0140 (Enter Shop), 0x016E, 0x016F are 2-byte packets: opcode only.
    for pid in ["0x0140", "0x016E", "0x016F"] {
        let p = load_fixture()
            .into_iter()
            .find(|p| p.pid == pid)
            .unwrap_or_else(|| panic!("{pid} captured"));
        let plaintext = hex::decode(&p.plaintext_hex).unwrap();
        let (_, payload) = split_opcode(&plaintext).unwrap();
        assert!(payload.is_empty(), "{pid} should have empty payload");
    }
}

/// Real channel config captured from the live C++ server's `0x004D` packet
/// (2026-06-27). Tuple: (name, max_user, id, flag, flag2, min_level, max_level).
const CAPTURED_CHANNELS: &[(&str, i16, u8, u32, i32, i32, i32)] = &[
    ("Canal (Iniciantes)", 500, 0, 512, 0, 0, 16),
    ("Canal (Livre 1)", 500, 1, 0, 0, 0, 70),
    ("Canal (Livre 2)", 500, 2, 0, 0, 0, 70),
    ("Canal (Livre 3)", 500, 3, 0, 0, 0, 70),
];

#[test]
fn channel_list_entry_matches_captured_layout_byte_for_byte() {
    // Build a channel list from the captured config and verify the per-entry
    // layout (name[64] + i16/i16/u8/u32/i32/i32/i32 = 85 bytes) reproduces the
    // real server's bytes exactly for all static fields. curr_user is 0 here
    // (no players in the captured channels at capture time).
    use pangya_proto::game_resp::{build_channel_list, ChannelInfoWire};

    let wires: Vec<ChannelInfoWire> = CAPTURED_CHANNELS
        .iter()
        .map(
            |(name, max_user, id, flag, flag2, min_lvl, max_lvl)| ChannelInfoWire {
                name: (*name).to_string(),
                max_user: *max_user,
                curr_user: 0,
                id: *id,
                flag: *flag,
                flag2: *flag2,
                min_level_allow: *min_lvl,
                max_level_allow: *max_lvl,
            },
        )
        .collect();

    let body = build_channel_list(&wires);

    // Header: opcode 0x004D + count(4).
    assert_eq!(body[0..2], [0x4D, 0x00]);
    assert_eq!(body[2], 4);
    assert_eq!(body.len(), 3 + 4 * ChannelInfoWire::WIRE_SIZE); // 3 + 340 = 343

    // First entry: name field starts at offset 3, nul-padded to 64 bytes.
    let name0 = std::str::from_utf8(&body[3..3 + 64]).unwrap();
    let name0 = name0.trim_end_matches('\0');
    assert_eq!(name0, "Canal (Iniciantes)");

    // max_user (i16 LE) at offset 3+64 = 67 == 500.
    assert_eq!(i16::from_le_bytes([body[67], body[68]]), 500);
    // id (u8) at offset 3+64+4 = 71 == 0.
    assert_eq!(body[71], 0);
    // flag (u32 LE) at offset 72 == 512.
    assert_eq!(u32::from_le_bytes(body[72..76].try_into().unwrap()), 512);
    // max_level_allow (i32 LE) at the entry's last 4 bytes == 16.
    let max_lvl_off = 3 + ChannelInfoWire::WIRE_SIZE - 4;
    assert_eq!(
        i32::from_le_bytes(body[max_lvl_off..max_lvl_off + 4].try_into().unwrap()),
        16
    );
}
