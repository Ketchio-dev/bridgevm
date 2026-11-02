//! Delayed GET replies cannot adopt bytes under another active host destination.

use super::*;

fn start_changed_get(rig: &mut Rig, name: &str, bytes: &[u8]) -> String {
    rig.guest.write(name, bytes);
    rig.run(1);
    let request = rig.h.queue.pop_front().expect("changed guest file fetch");
    assert!(matches!(&request, ServiceReq::ShareGet { name: requested } if requested == name));
    let wire = rig
        .h
        .service_req_line(&request, rig.now)
        .unwrap()
        .to_owned();
    let argument = wire.trim_end().strip_prefix("GET ").unwrap().to_owned();
    assert_eq!(
        String::from_utf8(base64_decode(&argument).unwrap()).unwrap(),
        format!("{ROOT}\\{name}")
    );
    rig.h.in_flight = Some((request, rig.now));
    rig.h.last_send = Some(rig.now);
    rig.h.last_alive = Some(rig.now);
    argument
}

fn reply(rig: &mut Rig, path: &str, bytes: &[u8]) {
    let packet = format!(
        "GETBEG {path} {} 1\nGETCHUNK 0 {}\nGETEND 1\n",
        bytes.len(),
        base64_encode(bytes)
    );
    rig.h.inbound_scratch.extend_from_slice(packet.as_bytes());
    rig.h.advance(rig.now, &mut |_| {
        panic!("reply unexpectedly sent another request")
    });
}

fn guest_entry(rig: &Rig, name: &str) -> LsEntry {
    let (_, bytes, mtime) = rig
        .guest
        .files
        .iter()
        .find(|(path, _, _)| path == name)
        .unwrap();
    LsEntry {
        name: name.into(),
        size: bytes.len() as u64,
        is_dir: false,
        mtime: format!("m{mtime}"),
    }
}

#[test]
fn different_path_reply_keeps_the_active_file_and_baseline_until_its_own_begin() {
    let name = "wanted.txt";
    let mut rig = Rig::synced("get-reply-identity", name, b"host baseline");
    let inode = rig.inode(name);
    let requested = start_changed_get(&mut rig, name, b"current guest version");
    let entry = guest_entry(&rig, name);
    let other = base64_encode(format!("{ROOT}\\other.txt").as_bytes());
    reply(&mut rig, &other, b"unrelated delayed reply");
    assert!(
        rig.host(name).as_deref() == Some(&b"host baseline"[..]),
        "different-path GET overwrote the active host destination"
    );
    assert_eq!(rig.inode(name), inode);
    assert!(
        matches!(rig.h.in_flight.as_ref(), Some((ServiceReq::ShareGet { name: active }, _)) if active == name)
    );
    assert!(
        rig.h
            .get_accum
            .as_ref()
            .is_some_and(|accum| accum.path == format!("{ROOT}\\other.txt")),
        "unmatched completion consumed its reassembly"
    );
    let plans = rig
        .h
        .share
        .as_mut()
        .unwrap()
        .engine
        .on_guest_listing_normalized([entry.clone()]);
    assert!(
        matches!(plans.as_slice(), [SyncAction::Get { name: fetch }] if fetch == name),
        "unmatched completion advanced the sync baseline"
    );
    reply(&mut rig, &requested, b"current guest version");
    assert!(rig.host(name).as_deref() == Some(&b"current guest version"[..]));
    assert!(rig.h.in_flight.is_none());
    assert!(rig
        .h
        .share
        .as_mut()
        .unwrap()
        .engine
        .on_guest_listing_normalized([entry])
        .is_empty());
}

#[test]
fn a_same_name_later_get_ignores_end_before_its_begin_and_then_recovers() {
    let name = "same.txt";
    let mut rig = Rig::synced("get-stray-end", name, b"baseline");
    let first = start_changed_get(&mut rig, name, b"first version");
    // A failed request may have already delivered a full body without END.
    let failed = format!(
        "GETBEG {first} 13 1\nGETCHUNK 0 {}\nERR GET interrupted\n",
        base64_encode(b"first version")
    );
    rig.h.inbound_scratch.extend_from_slice(failed.as_bytes());
    rig.h.advance(rig.now, &mut |_| {
        panic!("failed GET unexpectedly sent another request")
    });
    assert!(rig.h.in_flight.is_none());
    assert!(rig.host(name).as_deref() == Some(&b"baseline"[..]));
    let next = start_changed_get(&mut rig, name, b"next version");
    assert_eq!(first, next);
    let inode = rig.inode(name);
    rig.h.inbound_scratch.extend_from_slice(b"GETEND 1\n");
    rig.h.advance(rig.now, &mut |_| {
        panic!("stray END unexpectedly sent another request")
    });
    assert!(
        matches!(rig.h.in_flight.as_ref(), Some((ServiceReq::ShareGet { name: active }, _)) if active == name),
        "a stray END completed a new request without its BEGIN"
    );
    assert!(rig.host(name).as_deref() == Some(&b"baseline"[..]));
    assert_eq!(rig.inode(name), inode);
    reply(&mut rig, &next, b"next version");
    assert!(rig.host(name).as_deref() == Some(&b"next version"[..]));
    assert!(rig.h.in_flight.is_none());
}
