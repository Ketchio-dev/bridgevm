use super::*;

#[test]
fn self_export_keeps_original_source_without_pinning_replaced_generations() {
    let (s, media, _) = fixture("stream-source-lifetime");
    let mut media = media;
    let input = media.nvme_disk.as_ref().unwrap().path.clone();
    media.nvme_disk.as_mut().unwrap().snapshot_path = Some(input.clone());
    let original = s.path("original-alias");
    fs::hard_link(&input, &original).unwrap();
    let mut source = fs::File::open(&input).unwrap();
    let mut owner = acquire(&mut media).unwrap();
    let mut old_output: Option<PathBuf> = None;
    for generation in 0..8 {
        use std::io::{Seek, SeekFrom};
        source.seek(SeekFrom::Start(0)).unwrap();
        owner
            .export_snapshot(RuntimeMediaSlot::Primary, |out| io::copy(&mut source, out))
            .unwrap();
        owned(&original);
        owned(&input);
        if let Some(old) = old_output.take() {
            MediaLease::acquire([old.as_path()]).unwrap();
        }
        let alias = s.path(&format!("export-{generation}"));
        fs::hard_link(&input, &alias).unwrap();
        owned(&alias);
        owner
            .persist(RuntimeMediaSlot::Vars, b"updated-vars")
            .unwrap();
        owned(&original);
        owned(&alias);
        old_output = Some(alias);
    }
    drop(owner);
    MediaLease::acquire([original.as_path(), input.as_path()]).unwrap();
}
