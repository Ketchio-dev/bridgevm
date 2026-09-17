const COLOR_WORDS: usize = 1 << 18;
pub(super) struct ExactColorSet {
    words: Box<[u64]>,
    len: usize,
}

impl ExactColorSet {
    pub(super) fn new() -> Self {
        Self {
            words: vec![0; COLOR_WORDS].into_boxed_slice(),
            len: 0,
        }
    }

    pub(super) fn insert_bgr(&mut self, bgr: &[u8]) {
        let color = usize::from(bgr[2]) << 16 | usize::from(bgr[1]) << 8 | usize::from(bgr[0]);
        let mask = 1u64 << (color & 63);
        let word = &mut self.words[color >> 6];
        if *word & mask == 0 {
            *word |= mask;
            self.len += 1;
        }
    }

    pub(super) fn len(&self) -> usize {
        self.len
    }
}
