const FOUR_ALPHA_BYTES: u128 = 0x00ff_ffff_00ff_ffff_00ff_ffff_00ff_ffff;

pub(crate) fn convert_bgr_row_to_xrgb(source: &[u8], target: &mut [u8]) {
    debug_assert_eq!(source.len(), target.len());
    debug_assert_eq!(source.len() % 4, 0);

    let mut source_words = source.chunks_exact(16);
    let mut target_words = target.chunks_exact_mut(16);
    for (source_word, target_word) in source_words.by_ref().zip(target_words.by_ref()) {
        let word = u128::from_le_bytes(source_word.try_into().expect("16-byte source chunk"));
        target_word.copy_from_slice(&(word & FOUR_ALPHA_BYTES).to_le_bytes());
    }

    for (source_pixel, target_pixel) in source_words
        .remainder()
        .chunks_exact(4)
        .zip(target_words.into_remainder().chunks_exact_mut(4))
    {
        target_pixel.copy_from_slice(&[source_pixel[0], source_pixel[1], source_pixel[2], 0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_a_full_word_and_scalar_tail() {
        let source: Vec<_> = (1..=20).collect();
        let mut target = vec![0xff; source.len()];

        convert_bgr_row_to_xrgb(&source, &mut target);

        assert_eq!(
            target,
            [1, 2, 3, 0, 5, 6, 7, 0, 9, 10, 11, 0, 13, 14, 15, 0, 17, 18, 19, 0]
        );
    }
}
