//! Parser incremental de OSC 7 sobre los bytes del PTY, sin modificarlos.

#[cfg(test)]
mod tests {
    use super::{Osc7Tracker, MAX_PAYLOAD};
    use tuki_core::ports::CwdTracker;
    use tuki_core::remote_path::RemotePath;

    fn assert_path(bytes: &[u8], expected: &str) {
        let mut tracker = Osc7Tracker::new();
        assert_eq!(tracker.feed(bytes), Some(RemotePath::new(expected).unwrap()));
    }

    fn assert_none(bytes: &[u8]) {
        let mut tracker = Osc7Tracker::new();
        assert_eq!(tracker.feed(bytes), None);
    }

    #[test]
    fn should_return_path_when_osc7_uses_bel_terminator() {
        assert_path(b"\x1b]7;file://host/home/tuki\x07", "/home/tuki");
    }

    #[test]
    fn should_return_path_when_osc7_uses_st_terminator() {
        assert_path(b"\x1b]7;file:///home/tuki\x1b\\", "/home/tuki");
    }

    #[test]
    fn should_return_root_when_uri_has_empty_host() {
        assert_path(b"\x1b]7;file:///\x07", "/");
    }

    #[test]
    fn should_return_none_until_terminator_when_sequence_is_partial() {
        let mut tracker = Osc7Tracker::new();
        for chunk in [b"\x1b".as_slice(), b"]7;file://h".as_slice(), b"ost/tmp".as_slice()] {
            assert_eq!(tracker.feed(chunk), None);
        }
    }

    #[test]
    fn should_return_path_when_sequence_split_in_two_at_prefix() {
        let mut tracker = Osc7Tracker::new();
        assert_eq!(tracker.feed(b"\x1b]7;file"), None);
        assert_eq!(
            tracker.feed(b"://host/home/tuki\x07"),
            Some(RemotePath::new("/home/tuki").unwrap())
        );
    }

    #[test]
    fn should_return_path_when_sequence_split_in_three_at_terminator() {
        let mut tracker = Osc7Tracker::new();
        assert_eq!(tracker.feed(b"\x1b]7;file://host/ho"), None);
        assert_eq!(tracker.feed(b"me/tuki\x1b"), None);
        assert_eq!(
            tracker.feed(b"\\"),
            Some(RemotePath::new("/home/tuki").unwrap())
        );
    }

    #[test]
    fn should_return_last_path_when_two_sequences_in_same_chunk() {
        assert_path(
            b"\x1b]7;file:///a\x07ls\n\x1b]7;file:///b\x07",
            "/b",
        );
    }

    #[test]
    fn should_keep_last_valid_path_when_following_sequence_is_invalid() {
        let mut tracker = Osc7Tracker::new();
        tracker.feed(b"\x1b]7;file:///a\x07");
        assert_eq!(
            tracker.feed(b"\x1b]7;http://host/b\x07"),
            Some(RemotePath::new("/a").unwrap())
        );
    }

    #[test]
    fn should_decode_percent_encoded_space_when_payload_has_escapes() {
        assert_path(b"\x1b]7;file:///home/my%20dir\x07", "/home/my dir");
    }

    #[test]
    fn should_decode_utf8_when_payload_has_multibyte_escapes() {
        assert_path(b"\x1b]7;file:///tmp/caf%C3%A9\x07", "/tmp/café");
    }

    #[test]
    fn should_ignore_title_csi_and_text_when_followed_by_osc7() {
        assert_path(
            b"prompt\x1b[2J\x1b]0;title\x07ls\n\x1b]7;file:///srv\x07",
            "/srv",
        );
    }

    #[test]
    fn should_return_none_when_feeding_title_or_csi_only() {
        assert_none(b"\x1b]0;title\x07");
        assert_none(b"\x1b[2J");
        assert_none(b"prompt $ ");
    }

    #[test]
    fn should_discard_oversized_payload_when_exceeding_max_and_recover() {
        let mut stream = b"\x1b]7;file:///tmp/".to_vec();
        stream.extend(vec![b'a'; MAX_PAYLOAD]);
        stream.push(0x07);
        stream.extend_from_slice(b"\x1b]7;file:///ok\x07");
        assert_path(&stream, "/ok");
    }

    #[test]
    fn should_accept_payload_when_exactly_at_max() {
        let tail = MAX_PAYLOAD - b"file:///".len();
        let mut stream = b"\x1b]7;file:///".to_vec();
        stream.extend(vec![b'a'; tail]);
        stream.push(0x07);
        let expected = format!("/{}", "a".repeat(tail));
        assert_path(&stream, &expected);
    }

    #[test]
    fn should_return_none_when_scheme_is_not_file() {
        assert_none(b"\x1b]7;ssh://host/tmp\x07");
    }

    #[test]
    fn should_return_none_when_percent_escape_is_invalid() {
        assert_none(b"\x1b]7;file:///tmp/%ZZ\x07");
        assert_none(b"\x1b]7;file:///tmp/%2\x07");
    }

    #[test]
    fn should_return_none_when_decoded_path_is_not_utf8() {
        assert_none(b"\x1b]7;file:///tmp/%FF\x07");
    }

    #[test]
    fn should_return_none_when_uri_has_no_path() {
        assert_none(b"\x1b]7;file://host\x07");
    }
}
