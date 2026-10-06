use crate::fwcfg::GuestMemoryMut;

struct FailingWrite {
    failure: usize,
    writes: Vec<u64>,
}
impl GuestMemoryMut for FailingWrite {
    fn read_bytes(&self, _: u64, _: usize) -> Option<Vec<u8>> {
        None
    }
    fn read_into(&self, _: u64, bytes: &mut [u8]) -> bool {
        bytes.fill(0);
        true
    }
    fn write_bytes(&mut self, address: u64, _: &[u8]) -> bool {
        self.writes.push(address);
        self.writes.len() != self.failure
    }
}
#[test]
fn failed_used_element_write_does_not_publish_the_index() {
    for failure in [1, 2] {
        let mut mem = FailingWrite {
            failure,
            writes: Vec::new(),
        };
        super::super::address::write_used(&mut mem, 0x1000, 1, 7, 4);
        assert!(
            !mem.writes.contains(&0x1002),
            "failed element must not publish used index"
        );
        assert_eq!(mem.writes.len(), failure);
    }
}
#[test]
fn successful_used_element_is_published_before_its_index() {
    let mut mem = FailingWrite {
        failure: usize::MAX,
        writes: Vec::new(),
    };
    super::super::address::write_used(&mut mem, 0x1000, 1, 7, 4);
    assert_eq!(mem.writes, [0x1004, 0x1008, 0x1002]);
}
