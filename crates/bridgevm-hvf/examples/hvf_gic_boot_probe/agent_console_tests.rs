//! Agent console protocol and service regression tests.

use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn harness() -> AgentConsoleHarness {
        AgentConsoleHarness {
            start: Instant::now(),
            timeout: Duration::from_secs(1),
            scripted_test: true,
            periodic_test: false,
            framer: LineFramer::new(),
            inbound_scratch: Vec::new(),
            line_scratch: Vec::new(),
            state: AgentConsoleState::WaitingOut { index: 0 },
            commands: vec!["GET Zm9v".to_string()],
            last_ping: None,
            get_accum: None,
            out_accum: None,
            service: false,
            clipsync: false,
            clip_interval: Duration::from_millis(1000),
            clip_max_bytes: 8 * 1024,
            queue: VecDeque::new(),
            in_flight: None,
            last_synced: None,
            pasteboard: None,
            ctl_path: None,
            ctl_offset: 0,
            ctl_framer: LineFramer::new(),
            ctl_read_scratch: Vec::new(),
            ctl_line_scratch: Vec::new(),
            service_line_scratch: String::new(),
            clip_crlf_scratch: String::new(),
            share_guest_path_scratch: String::new(),
            ctl_last_poll: None,
            last_clip_poll: None,
            last_send: None,
            last_alive: None,
            share: None,
            share_old_agent_warned: false,
        }
    }

    #[test]
    fn desktop_ready_requires_agent_handshake() {
        let mut h = harness();
        h.state = AgentConsoleState::WaitingReady;
        assert!(!h.desktop_ready());
        h.state = AgentConsoleState::WaitingPong;
        assert!(h.desktop_ready());
        h.state = AgentConsoleState::TimedOut;
        assert!(!h.desktop_ready());
    }

    #[test]
    fn only_scripted_harness_requires_per_exit_ticks() {
        let mut h = harness();
        assert!(h.per_exit_tick_needed());

        h.periodic_test = true;
        assert!(!h.per_exit_tick_needed());
        assert!(h.service_wake_needed());

        h.periodic_test = false;
        h.scripted_test = false;
        assert!(!h.per_exit_tick_needed());
    }

    fn queue_kinds(h: &AgentConsoleHarness) -> Vec<String> {
        h.queue
            .iter()
            .map(|r| match r {
                ServiceReq::ClipPoll => "poll".to_string(),
                ServiceReq::ClipPush(t) => format!("push:{t}"),
                ServiceReq::Ctl(c) => format!("ctl:{c}"),
                ServiceReq::Ping => "ping".to_string(),
                ServiceReq::ShareLs => "share-ls".to_string(),
                ServiceReq::ShareGet { name } => format!("share-get:{name}"),
                ServiceReq::SharePut { name, bytes, .. } => {
                    format!("share-put:{name}:{}", String::from_utf8_lossy(bytes))
                }
                ServiceReq::ShareDel { name, direction } => {
                    format!("share-del:{direction:?}:{name}")
                }
            })
            .collect()
    }

    fn ctl_cmds(h: &AgentConsoleHarness) -> Vec<String> {
        h.queue
            .iter()
            .filter_map(|r| match r {
                ServiceReq::Ctl(c) => Some(c.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn service_req_lines_reuse_output_scratch() {
        let mut h = harness();
        let now = Instant::now();
        let long = ServiceReq::ClipPush("x".repeat(256));
        assert!(h.write_service_req_line(&long, now));
        assert!(h.service_line_scratch.starts_with("CLIPSET "));
        let ptr = h.service_line_scratch.as_ptr();
        let capacity = h.service_line_scratch.capacity();
        let crlf_ptr = h.clip_crlf_scratch.as_ptr();
        let crlf_capacity = h.clip_crlf_scratch.capacity();

        let short = ServiceReq::Ctl("whoami".into());
        assert!(h.write_service_req_line(&short, now));
        assert!(h.service_line_scratch.starts_with("RUN "));
        assert_eq!(h.service_line_scratch.as_ptr(), ptr);
        assert_eq!(h.service_line_scratch.capacity(), capacity);

        let shorter = ServiceReq::ClipPush("y".into());
        assert!(h.write_service_req_line(&shorter, now));
        assert_eq!(h.clip_crlf_scratch.as_ptr(), crlf_ptr);
        assert_eq!(h.clip_crlf_scratch.capacity(), crlf_capacity);
    }

    #[test]
    fn share_service_lines_reuse_guest_path_scratch() {
        let mut h = harness();
        h.share = Some(test_share_state("path-scratch"));
        let now = Instant::now();

        let long = ServiceReq::ShareGet {
            name: "long/nested/path/that/should/grow/the/buffer.txt".into(),
        };
        assert!(h.write_service_req_line(&long, now));
        assert_eq!(
            h.share_guest_path_scratch,
            "C:\\share\\long\\nested\\path\\that\\should\\grow\\the\\buffer.txt"
        );
        let path_ptr = h.share_guest_path_scratch.as_ptr();
        let path_capacity = h.share_guest_path_scratch.capacity();

        let short = ServiceReq::ShareDel {
            name: "x.txt".into(),
            direction: ShareDelDirection::HostToGuest,
        };
        assert!(h.write_service_req_line(&short, now));
        assert_eq!(h.share_guest_path_scratch, "C:\\share\\x.txt");
        assert_eq!(h.share_guest_path_scratch.as_ptr(), path_ptr);
        assert_eq!(h.share_guest_path_scratch.capacity(), path_capacity);
    }

    #[test]
    fn is_raw_verb_covers_protocol_verbs_only() {
        for v in [
            "CLIPGET", "CLIPSET", "LS", "LSR", "GET", "PUT", "PUTBEG", "PUTCHUNK", "PUTEND", "DEL",
            "PING",
        ] {
            assert!(is_raw_verb(v), "{v} should be raw");
        }
        for v in ["whoami", "PS", "RUN", "ipconfig", ""] {
            assert!(!is_raw_verb(v), "{v} should be RUN-wrapped");
        }
    }

    #[test]
    fn reassembles_get_chunks_across_lines() {
        let mut h = harness();
        let payload: Vec<u8> = (0u8..=255).cycle().take(600).collect();
        let path_b64 = base64_encode(b"C:\\Windows\\Temp\\x.bin");
        h.begin_get(&format!("{path_b64} {} 3", payload.len()));
        for (seq, chunk) in payload.chunks(256).enumerate() {
            h.accum_get_chunk(&format!("{seq} {}", base64_encode(chunk)));
        }
        let accum = h.get_accum.as_ref().expect("accum present before end");
        assert_eq!(accum.bytes, payload);
        assert_eq!(accum.chunks_seen, 3);
        assert_eq!(accum.total, payload.len());
        assert_eq!(accum.path, "C:\\Windows\\Temp\\x.bin");
    }

    #[test]
    fn empty_get_reassembles_to_zero_bytes() {
        let mut h = harness();
        h.begin_get(&format!("{} 0 0", base64_encode(b"C:\\empty.txt")));
        // no GETCHUNK lines for an empty file
        let accum = h.get_accum.as_ref().unwrap();
        assert_eq!(accum.total, 0);
        assert!(accum.bytes.is_empty());
        assert_eq!(accum.nchunks, 0);
    }

    #[test]
    fn get_accum_ignores_bad_chunk_base64_but_keeps_good_ones() {
        let mut h = harness();
        h.begin_get(&format!("{} 6 2", base64_encode(b"C:\\f")));
        h.accum_get_chunk(&format!("0 {}", base64_encode(b"foo")));
        h.accum_get_chunk("1 not-valid-b64!!!"); // bad -> ignored
        h.accum_get_chunk(&format!("2 {}", base64_encode(b"bar")));
        let accum = h.get_accum.as_ref().unwrap();
        assert_eq!(accum.bytes, b"foobar");
        assert_eq!(accum.chunks_seen, 2);
    }

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
        let mut lines = Vec::new();

        framer.push_into(b"REA", &mut lines);
        assert!(lines.is_empty());
        framer.push_into(b"DY host\r\nPONG\nOUT", &mut lines);
        assert_eq!(lines.as_slice(), ["READY host", "PONG"]);
        lines.clear();
        framer.push_into(b" 0 Zm9v", &mut lines);
        assert!(lines.is_empty());
        framer.push_into(b"\n", &mut lines);
        assert_eq!(lines.as_slice(), ["OUT 0 Zm9v"]);
    }

    #[test]
    fn line_framer_push_into_reuses_output_vec_and_keeps_tail() {
        let mut framer = LineFramer::new();
        let mut lines = Vec::with_capacity(4);
        let initial_capacity = lines.capacity();

        framer.push_into(b"one\r\ntwo\npartial", &mut lines);
        assert_eq!(lines.as_slice(), ["one", "two"]);
        assert_eq!(lines.capacity(), initial_capacity);

        lines.clear();
        framer.push_into(b"-done\r\n", &mut lines);
        assert_eq!(lines.as_slice(), ["partial-done"]);
        assert_eq!(lines.capacity(), initial_capacity);
    }

    #[test]
    fn bounded_line_framer_discards_oversized_line_and_resynchronizes() {
        let mut framer = LineFramer::new();
        let mut lines = Vec::new();

        assert_eq!(framer.push_bounded_into(b"1234", &mut lines, 4), 0);
        assert_eq!(framer.push_bounded_into(b"5", &mut lines, 4), 0);
        assert_eq!(framer.push_bounded_into(b"6", &mut lines, 4), 1);
        assert!(lines.is_empty());
        assert!(framer.pending.is_empty());
        assert!(framer.discarding_oversized_line);

        assert_eq!(
            framer.push_bounded_into(b"discarded\nok\r\n", &mut lines, 4),
            0
        );
        assert_eq!(lines, ["ok"]);
        assert!(!framer.discarding_oversized_line);

        lines.clear();
        assert_eq!(framer.push_bounded_into(b"1234\r\n", &mut lines, 4), 0);
        assert_eq!(lines, ["1234"]);
    }

    #[test]
    fn normalize_clip_folds_crlf_to_lf() {
        assert_eq!(normalize_clip(""), "");
        assert_eq!(normalize_clip("a\r\nb"), "a\nb");
        assert_eq!(normalize_clip("a\nb"), "a\nb");
        assert_eq!(normalize_clip("a\r\nb\n"), "a\nb\n");
        assert_eq!(normalize_clip("mix\r\nof\nlines\r\n"), "mix\nof\nlines\n");
    }

    #[test]
    fn to_guest_crlf_expands_lf_without_doubling() {
        assert_eq!(to_guest_crlf(""), "");
        assert_eq!(to_guest_crlf("a\nb"), "a\r\nb");
        // Already-CRLF input must not become \r\r\n.
        assert_eq!(to_guest_crlf("a\r\nb"), "a\r\nb");
        assert_eq!(to_guest_crlf("a\r\nb\nc"), "a\r\nb\r\nc");
        assert_eq!(to_guest_crlf("trailing\n"), "trailing\r\n");
        assert_eq!(
            to_guest_crlf("mixed\r\nand\nlf\r\n"),
            "mixed\r\nand\r\nlf\r\n"
        );
    }

    #[test]
    fn to_guest_crlf_len_matches_conversion_without_allocating_result() {
        for text in [
            "",
            "plain",
            "a\nb",
            "a\r\nb",
            "a\r\nb\nc",
            "trailing\n",
            "mixed\r\nand\nlf\r\n",
            "lone\rcarriage",
            "emoji \u{1f642}\n",
        ] {
            assert_eq!(to_guest_crlf_len(text), to_guest_crlf(text).len());
        }
    }

    #[test]
    fn guest_clip_decision_syncs_only_new_normalized_text() {
        // Empty -> no-op.
        assert_eq!(guest_clip_decision(&None, ""), None);
        // New text -> adopt the normalized (LF) form.
        assert_eq!(
            guest_clip_decision(&None, "a\r\nb"),
            Some("a\nb".to_string())
        );
        // Same content as last_synced (either line-ending) -> no-op.
        let last = Some("a\nb".to_string());
        assert_eq!(guest_clip_decision(&last, "a\r\nb"), None);
        assert_eq!(guest_clip_decision(&last, "a\nb"), None);
        // Genuinely different -> adopt.
        assert_eq!(guest_clip_decision(&last, "c"), Some("c".to_string()));
    }

    #[test]
    fn clip_push_enqueue_coalesces_and_spares_inflight() {
        let mut h = harness();
        // A push already on the wire must NOT be touched by a newer host change.
        h.in_flight = Some((ServiceReq::ClipPush("inflight".into()), Instant::now()));
        h.queue.push_back(ServiceReq::ClipPush("old".into()));
        h.queue.push_back(ServiceReq::Ctl("dir".into()));

        h.enqueue_clip_push("new".into());

        // "old" dropped, "new" appended after the untouched Ctl; latest wins.
        assert_eq!(queue_kinds(&h), vec!["ctl:dir", "push:new"]);
        match h.in_flight.as_ref() {
            Some((ServiceReq::ClipPush(t), _)) => assert_eq!(t, "inflight"),
            _ => panic!("in-flight push was disturbed"),
        }
    }

    #[test]
    fn ctl_tailer_frames_partial_appends_and_resets_on_truncation() {
        use std::io::Write;

        let path = std::env::temp_dir().join(format!("bvagent-ctl-{}.txt", std::process::id()));
        let path_str = path.to_string_lossy().into_owned();
        let mut h = harness();
        h.ctl_path = Some(path_str);

        // A line split across two appends must reassemble.
        std::fs::write(&path, b"whoami\npar").unwrap();
        h.ingest_ctl();
        assert_eq!(ctl_cmds(&h), vec!["whoami"]);

        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"tial\nCLIPGET\n").unwrap();
        drop(f);
        h.ingest_ctl();
        assert_eq!(ctl_cmds(&h), vec!["whoami", "partial", "CLIPGET"]);
        let read_capacity = h.ctl_read_scratch.capacity();
        let line_capacity = h.ctl_line_scratch.capacity();
        assert!(read_capacity > 0);
        assert!(line_capacity > 0);

        h.ingest_ctl();
        assert_eq!(h.ctl_read_scratch.capacity(), read_capacity);
        assert_eq!(h.ctl_line_scratch.capacity(), line_capacity);

        // Truncation (shrink below offset) restarts from the top.
        std::fs::write(&path, b"ver\n").unwrap();
        h.ingest_ctl();
        assert_eq!(ctl_cmds(&h), vec!["whoami", "partial", "CLIPGET", "ver"]);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_ctl_file_yields_no_commands() {
        let path =
            std::env::temp_dir().join(format!("bvagent-ctl-absent-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut h = harness();
        h.ctl_path = Some(path.to_string_lossy().into_owned());
        h.ingest_ctl(); // must not panic / spam
        assert!(h.queue.is_empty());
    }

    #[test]
    fn empty_commands_with_service_flag_enters_service_state() {
        let mut h = harness();
        h.commands = vec![];
        h.service = true;
        h.state = AgentConsoleState::WaitingReady;
        let line = h.next_command_line(0, Instant::now());
        assert!(line.is_none());
        assert!(matches!(h.state, AgentConsoleState::Service));
        assert!(h.last_send.is_some(), "heartbeat clock anchored at entry");
    }

    #[test]
    fn empty_commands_without_service_flag_is_done() {
        let mut h = harness();
        h.commands = vec![];
        h.service = false;
        let line = h.next_command_line(0, Instant::now());
        assert!(line.is_none());
        assert!(matches!(h.state, AgentConsoleState::Done));
    }

    #[test]
    fn clip_poll_reply_updates_last_synced_without_pasteboard() {
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.clipsync = true;
        h.in_flight = Some((ServiceReq::ClipPoll, Instant::now()));

        // Guest clipboard "hello\r\n" -> normalized "hello\n" adopted; no panic
        // even though pasteboard is None (the host set is simply skipped).
        let b64 = base64_encode(b"hello\r\n");
        h.handle_service_reply(&format!("CLIP {b64}"), None, None, Instant::now());

        assert_eq!(h.last_synced.as_deref(), Some("hello\n"));
        assert!(h.in_flight.is_none(), "reply completes the in-flight poll");
    }

    #[test]
    fn clip_poll_not_enqueued_without_clipsync() {
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.clipsync = false; // no pasteboard, clipsync off

        h.service_enqueue(Instant::now());

        assert!(
            !h.queue.iter().any(|r| matches!(r, ServiceReq::ClipPoll)),
            "ClipPoll must never be queued when clipsync is off"
        );
    }

    #[test]
    fn overdue_service_request_keeps_wire_alignment() {
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        let sent_at = Instant::now();
        h.in_flight = Some((ServiceReq::Ctl("slow-command".into()), sent_at));
        h.queue.push_back(ServiceReq::Ctl("next-command".into()));

        assert_eq!(
            h.note_service_overdue(sent_at + SERVICE_OVERDUE_INTERVAL),
            Some("ctl")
        );
        assert!(matches!(
            h.in_flight.as_ref().map(|(request, _)| request),
            Some(ServiceReq::Ctl(command)) if command == "slow-command"
        ));
        assert!(
            matches!(h.queue.front(), Some(ServiceReq::Ctl(command)) if command == "next-command")
        );
    }

    #[test]
    fn fresh_ready_is_the_service_resynchronization_point() {
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.in_flight = Some((ServiceReq::Ctl("lost-command".into()), Instant::now()));
        h.get_accum = Some(GetAccum {
            path: "lost.bin".into(),
            total: 1,
            nchunks: 1,
            bytes: vec![1],
            chunks_seen: 1,
        });
        h.out_accum = Some(OutAccum {
            exit_code: 0,
            total: 1,
            nchunks: 1,
            bytes: vec![1],
            chunks_seen: 1,
            valid: true,
        });

        h.handle_service_reply("READY rebooted-host", None, None, Instant::now());

        assert!(h.in_flight.is_none());
        assert!(h.get_accum.is_none());
        assert!(h.out_accum.is_none());
    }

    #[test]
    fn chunked_command_output_keeps_lockstep_until_verified_end() {
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.in_flight = Some((ServiceReq::Ctl("large-command".into()), Instant::now()));
        let first = vec![b'a'; COMMAND_OUTPUT_CHUNK_BYTES];
        let second = b"second";
        let total = first.len() + second.len();

        h.handle_service_reply(&format!("OUTBEG 0 {total} 2"), None, None, Instant::now());
        assert!(h.in_flight.is_some());
        assert!(h.out_accum.is_some());
        h.handle_service_reply(
            &format!("OUTCHUNK 0 {}", base64_encode(&first)),
            None,
            None,
            Instant::now(),
        );
        assert!(h.in_flight.is_some());
        h.handle_service_reply(
            &format!("OUTCHUNK 1 {}", base64_encode(second)),
            None,
            None,
            Instant::now(),
        );
        assert!(h.in_flight.is_some());
        h.handle_service_reply("OUTEND 2", None, None, Instant::now());

        assert!(h.in_flight.is_none());
        assert!(h.out_accum.is_none());
    }

    #[test]
    fn oversized_chunked_command_output_is_rejected_without_large_reservation() {
        let mut h = harness();
        h.begin_out(&format!("0 {} 1", MAX_COMMAND_OUTPUT_BYTES + 1));

        let accum = h.out_accum.as_ref().unwrap();
        assert!(!accum.valid);
        assert_eq!(accum.bytes.capacity(), 0);
    }

    #[test]
    fn malformed_chunk_offsets_fail_closed_without_unsigned_underflow() {
        let mut h = harness();
        h.out_accum = Some(OutAccum {
            exit_code: 0,
            total: 1,
            nchunks: 3,
            bytes: Vec::new(),
            chunks_seen: 2,
            valid: true,
        });

        // The final chunk's nominal offset is 49,152 while total is one byte.
        // This must poison the envelope instead of subtracting with wrap/panic.
        h.accum_out_chunk("2 YQ==");

        let accum = h.out_accum.as_ref().unwrap();
        assert!(!accum.valid);
        assert!(accum.bytes.is_empty());
    }

    #[test]
    fn guest_agent_emits_chunked_output_and_retries_partial_writes() {
        let script = include_str!("../../../../scripts/win-assets/bvagent.ps1");
        for contract in [
            "function Write-CommandResult",
            "OUTBEG ",
            "OUTCHUNK ",
            "OUTEND ",
            "$totalWritten += [int]$written",
            "$remaining = $suffix",
        ] {
            assert!(
                script.contains(contract),
                "guest agent is missing protocol contract: {contract}"
            );
        }
    }

    #[test]
    fn parse_share_spec_accepts_windows_guest_paths() {
        assert_eq!(
            parse_share_spec("a::C:\\x"),
            Some(("a".to_string(), "C:\\x".to_string()))
        );
        assert_eq!(parse_share_spec("a:C:\\x"), None);
        assert_eq!(parse_share_spec("::C:\\x"), None);
        assert_eq!(parse_share_spec("a::"), None);
    }

    #[test]
    fn share_ls_not_enqueued_while_share_request_pending() {
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.last_send = Some(Instant::now());
        h.share = Some(test_share_state("pending"));
        h.in_flight = Some((
            ServiceReq::ShareGet {
                name: "x.txt".into(),
            },
            Instant::now(),
        ));

        h.service_enqueue(Instant::now());

        assert!(
            !h.queue.iter().any(|r| matches!(r, ServiceReq::ShareLs)),
            "ShareLs must wait until all Share requests leave the system"
        );

        h.in_flight = None;
        h.queue.push_back(share_put_req(
            "queued.txt".into(),
            b"queued".to_vec(),
            share_sync::fnv1a64(b"queued"),
        ));
        h.service_enqueue(Instant::now());
        assert_eq!(
            h.queue
                .iter()
                .filter(|r| matches!(r, ServiceReq::ShareLs))
                .count(),
            0
        );
    }

    #[test]
    fn share_put_captures_bytes_at_enqueue_time() {
        let dir = std::env::temp_dir().join(format!(
            "bvagent-share-capture-{}-{}",
            std::process::id(),
            "file"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.txt");
        std::fs::write(&path, b"first").unwrap();

        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.last_send = Some(Instant::now());
        h.share = Some(ShareState {
            engine: ShareSync::new(512),
            host_dir: dir.clone(),
            guest_dir: "C:\\share".into(),
            interval: Duration::from_millis(500),
            last_poll: None,
            host_skip_seen: HashSet::new(),
            guest_ls_scratch: Vec::new(),
            host_scan_scratch: Vec::new(),
        });

        h.service_enqueue(Instant::now());
        std::fs::write(&path, b"second").unwrap();

        let put = h
            .queue
            .iter()
            .find_map(|req| match req {
                ServiceReq::SharePut {
                    name, bytes, hash, ..
                } => Some((name.clone(), bytes.clone(), *hash)),
                _ => None,
            })
            .expect("SharePut queued");
        assert_eq!(put.0, "x.txt");
        assert_eq!(put.1, b"first");
        assert_eq!(put.2, share_sync::fnv1a64(b"first"));
        assert!(
            !h.queue.iter().any(|r| matches!(r, ServiceReq::ShareLs)),
            "ShareLs waits until the captured SharePut leaves the queue"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recursive_host_scan_finds_nested_files_and_skips_symlinks() {
        let dir =
            std::env::temp_dir().join(format!("bvagent-share-recursive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub/dir")).unwrap();
        std::fs::write(dir.join("sub/dir/x.txt"), b"x").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.join("sub/dir/x.txt"), dir.join("link.txt")).unwrap();

        let mut share = ShareState {
            engine: ShareSync::new(512),
            host_dir: dir.clone(),
            guest_dir: "C:\\share".into(),
            interval: Duration::from_millis(500),
            last_poll: None,
            host_skip_seen: HashSet::new(),
            guest_ls_scratch: Vec::new(),
            host_scan_scratch: Vec::new(),
        };
        scan_share_host_dir(&mut share);
        assert_eq!(share.host_scan_scratch.len(), 1);
        assert_eq!(share.host_scan_scratch[0].name, "sub/dir/x.txt");
        let capacity = share.host_scan_scratch.capacity();
        assert!(capacity > 0);

        scan_share_host_dir(&mut share);
        assert_eq!(share.host_scan_scratch.capacity(), capacity);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn host_scan_rejects_literal_backslash_names_that_windows_would_split() {
        let dir =
            std::env::temp_dir().join(format!("bvagent-share-backslash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a\\b.txt"), b"ambiguous").unwrap();

        let mut share = ShareState {
            engine: ShareSync::new(512),
            host_dir: dir.clone(),
            guest_dir: "C:\\share".into(),
            interval: Duration::from_millis(500),
            last_poll: None,
            host_skip_seen: HashSet::new(),
            guest_ls_scratch: Vec::new(),
            host_scan_scratch: Vec::new(),
        };

        scan_share_host_dir(&mut share);
        assert!(share.host_scan_scratch.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn share_put_small_payload_uses_legacy_single_line() {
        let mut h = harness();
        h.share = Some(test_share_state("small-put"));
        let req = share_put_req(
            "sub/x.txt".into(),
            b"small".to_vec(),
            share_sync::fnv1a64(b"small"),
        );
        let line = h.service_req_line(&req, Instant::now()).unwrap();
        assert!(line.starts_with("PUT "));
        assert!(!line.starts_with("PUTBEG "));
        match req {
            ServiceReq::SharePut { phase, .. } => assert_eq!(phase, SharePutPhase::Legacy),
            _ => panic!("expected SharePut"),
        }
    }

    #[test]
    fn share_put_chunked_sequence_restamps_each_step() {
        let mut h = harness();
        h.share = Some(test_share_state("chunk-put"));
        let bytes = vec![7u8; SHARE_PUT_CHUNK_BYTES * 2 + 1];
        let hash = share_sync::fnv1a64(&bytes);
        let req = share_put_req("big.bin".into(), bytes, hash);
        let t0 = Instant::now();
        let line = h.service_req_line(&req, t0).unwrap();
        assert!(line.starts_with("PUTBEG "));
        h.in_flight = Some((req, t0));

        let t1 = t0 + Duration::from_millis(1);
        h.handle_share_put_reply("OK PUTBEG", None, None, t1);
        assert!(h.service_line_scratch.starts_with("PUTCHUNK 0 "));
        let chunk_ptr = h.service_line_scratch.as_ptr();
        let chunk_capacity = h.service_line_scratch.capacity();
        match h.in_flight.as_ref().unwrap() {
            (
                ServiceReq::SharePut {
                    next_chunk, phase, ..
                },
                sent_at,
            ) => {
                assert_eq!(*next_chunk, 1);
                assert_eq!(*phase, SharePutPhase::Chunk);
                assert_eq!(*sent_at, t1);
            }
            _ => panic!("expected SharePut"),
        }

        let t2 = t0 + Duration::from_millis(2);
        h.handle_share_put_reply("OK PUTCHUNK 0", None, None, t2);
        assert!(h.service_line_scratch.starts_with("PUTCHUNK 1 "));
        assert_eq!(h.service_line_scratch.as_ptr(), chunk_ptr);
        assert_eq!(h.service_line_scratch.capacity(), chunk_capacity);
        match h.in_flight.as_ref().unwrap() {
            (
                ServiceReq::SharePut {
                    next_chunk, phase, ..
                },
                sent_at,
            ) => {
                assert_eq!(*next_chunk, 2);
                assert_eq!(*phase, SharePutPhase::Chunk);
                assert_eq!(*sent_at, t2);
            }
            _ => panic!("expected SharePut"),
        }

        let t3 = t0 + Duration::from_millis(3);
        h.handle_share_put_reply("OK PUTCHUNK 1", None, None, t3);
        assert!(h.service_line_scratch.starts_with("PUTCHUNK 2 "));
        assert_eq!(h.service_line_scratch.as_ptr(), chunk_ptr);
        assert_eq!(h.service_line_scratch.capacity(), chunk_capacity);
        match h.in_flight.as_ref().unwrap() {
            (
                ServiceReq::SharePut {
                    next_chunk, phase, ..
                },
                sent_at,
            ) => {
                assert_eq!(*next_chunk, 3);
                assert_eq!(*phase, SharePutPhase::Chunk);
                assert_eq!(*sent_at, t3);
            }
            _ => panic!("expected SharePut"),
        }

        let t4 = t0 + Duration::from_millis(4);
        h.handle_share_put_reply("OK PUTCHUNK 2", None, None, t4);
        assert_eq!(h.service_line_scratch, "PUTEND 3\n");
        assert_eq!(h.service_line_scratch.as_ptr(), chunk_ptr);
        assert_eq!(h.service_line_scratch.capacity(), chunk_capacity);
        match h.in_flight.as_ref().unwrap() {
            (ServiceReq::SharePut { phase, .. }, sent_at) => {
                assert_eq!(*phase, SharePutPhase::End);
                assert_eq!(*sent_at, t4);
            }
            _ => panic!("expected SharePut"),
        }

        h.handle_share_put_reply(
            &format!(
                "PUTOK {} {}",
                base64_encode(b"C:\\share\\big.bin"),
                SHARE_PUT_CHUNK_BYTES * 2 + 1
            ),
            None,
            None,
            t4 + Duration::from_millis(1),
        );
        assert!(h.in_flight.is_none());
    }

    #[test]
    fn old_agent_out_255_to_lsr_disables_share_once() {
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.share = Some(test_share_state("old-agent"));
        h.in_flight = Some((ServiceReq::ShareLs, Instant::now()));
        h.queue.push_back(ServiceReq::ShareGet {
            name: "x.txt".into(),
        });
        h.handle_service_reply(
            &format!("OUT 255 {}", base64_encode(b"unknown token: LSR")),
            None,
            None,
            Instant::now(),
        );
        assert!(h.share.is_none());
        assert!(h.share_old_agent_warned);
        assert!(h.queue.is_empty());

        h.share = Some(test_share_state("old-agent-again"));
        h.in_flight = Some((ServiceReq::ShareLs, Instant::now()));
        h.handle_service_reply(
            &format!("OUT 255 {}", base64_encode(b"unknown token: LSR")),
            None,
            None,
            Instant::now(),
        );
        assert!(h.share.is_none());
        assert!(h.share_old_agent_warned);
    }

    #[test]
    fn share_ls_reuses_guest_listing_scratch() {
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.share = Some(test_share_state("ls-scratch"));
        h.in_flight = Some((ServiceReq::ShareLs, Instant::now()));

        h.handle_service_reply(
            &format!(
                "LSOK {}",
                base64_encode(b"a.txt|1|0|2026-01-01T00:00:00.0000000Z\n")
            ),
            None,
            None,
            Instant::now(),
        );
        let capacity = h.share.as_ref().unwrap().guest_ls_scratch.capacity();
        assert!(capacity > 0);

        h.in_flight = Some((ServiceReq::ShareLs, Instant::now()));
        h.handle_service_reply(
            &format!("LSOK {}", base64_encode(b"")),
            None,
            None,
            Instant::now(),
        );
        assert_eq!(
            h.share.as_ref().unwrap().guest_ls_scratch.capacity(),
            capacity
        );
    }

    #[test]
    fn control_file_tail_read_is_bounded_per_poll() {
        let path = std::env::temp_dir().join(format!(
            "bvagent-control-bounded-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_CTL_READ_BYTES_PER_POLL * 3).unwrap();
        let mut bytes = Vec::new();

        assert!(read_ctl_appended_into(
            path.to_str().unwrap(),
            0,
            &mut bytes
        ));
        let _ = std::fs::remove_file(&path);

        assert_eq!(bytes.len() as u64, MAX_CTL_READ_BYTES_PER_POLL);
    }

    #[test]
    fn share_file_read_rejects_oversized_file_before_allocation() {
        let path = std::env::temp_dir().join(format!(
            "bvagent-share-bounded-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(512 * 1024 * 1024).unwrap();

        let error = read_share_file_bounded(&path, 4096).unwrap_err();
        let _ = std::fs::remove_file(&path);

        assert_eq!(error, ShareFileReadError::TooLarge(512 * 1024 * 1024));
    }

    fn test_share_state(label: &str) -> ShareState {
        let dir =
            std::env::temp_dir().join(format!("bvagent-share-{label}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        ShareState {
            engine: ShareSync::new(512),
            host_dir: dir,
            guest_dir: "C:\\share".into(),
            interval: Duration::from_millis(500),
            last_poll: None,
            host_skip_seen: HashSet::new(),
            guest_ls_scratch: Vec::new(),
            host_scan_scratch: Vec::new(),
        }
    }
}
