//! An unchanged guest folder is not sent again (READY `lssame`). The agent gives each listing it
//! makes a gen; the host keeps the last one it applied, with its bytes, and names that gen in
//! the next LSR. The agent answers `LSSAME <gen>` under the same rules under which it used to
//! send that very line again: its watcher saw no change below the root and the listing is under
//! a minute old (bvagent.ps1, share watch). At 100,000 entries that line is about 13 MB every
//! 2 s. The host then applies the kept listing again as it applied the line, so holds, marks and
//! a first listing's adoption behave as before.
//!
//! An agent without the marker never gets a gen: it reads the whole argument as the folder's
//! base64. A new agent process (READY) or session knows no gen, and only an applied listing is
//! kept: any other reply asks for a whole listing next time.

/// The agent's gens are 32 hex digits; a longer one, or another character, is not kept.
pub(in crate::agent_console) const MAX_LISTING_GEN_BYTES: usize = 64;

#[derive(Default)]
pub(in crate::agent_console) struct ListingSame {
    /// The agent answers LSSAME (READY lssame).
    offered: bool,
    /// The last applied listing: its gen and bytes.
    kept: Option<(String, Vec<u8>)>,
    /// Whole listings and LSSAME replies this session, and as last reported.
    counts: (u64, u64),
    reported: (u64, u64),
}

/// A gen the host keeps and sends back.
pub(in crate::agent_console) fn usable_gen(gen: &str) -> bool {
    !gen.is_empty() && gen.len() <= MAX_LISTING_GEN_BYTES && gen.bytes().all(|b| b.is_ascii_alphanumeric())
}

impl ListingSame {
    /// A READY: a new agent process, with or without LSSAME, knows no gen.
    pub(in crate::agent_console) fn ready(&mut self, offered: bool) {
        self.offered = offered;
        self.kept = None;
    }

    /// ` <gen>` for the next LSR, ` 0` when none is kept; nothing for an agent without LSSAME.
    pub(in crate::agent_console) fn write_lsr_gen(&self, line: &mut String) {
        if self.offered {
            line.push(' ');
            line.push_str(self.kept.as_ref().map_or("0", |(gen, _)| gen.as_str()));
        }
    }

    /// A whole listing (LSOK): nothing is kept until it is applied.
    pub(in crate::agent_console) fn note_whole(&mut self) {
        self.kept = None;
        self.counts.0 += 1;
    }

    /// A failed listing: the next LSR asks for a whole one.
    pub(in crate::agent_console) fn forget(&mut self) {
        self.kept = None;
    }

    /// An applied listing, kept for an LSSAME naming `gen`.
    pub(in crate::agent_console) fn keep(&mut self, gen: String, bytes: Vec<u8>) {
        debug_assert!(usable_gen(&gen));
        self.kept = Some((gen, bytes));
    }

    /// For an LSSAME naming `gen`: the kept listing, taken out until it is applied again and
    /// kept; None when another one is kept (the next LSR asks for a whole listing).
    pub(in crate::agent_console) fn take(&mut self, gen: &str) -> Option<(String, Vec<u8>)> {
        self.counts.1 += 1;
        self.kept.take().filter(|(kept, _)| kept == gen)
    }

    /// The counts since the last report, when they moved: the share's idle cost in run.log.
    pub(in crate::agent_console) fn report(&mut self) -> Option<String> {
        if self.counts == self.reported {
            return None;
        }
        self.reported = self.counts;
        let (lsok, lssame) = self.counts;
        Some(format!("BVAGENT SERVICE share-listings lsok={lsok} lssame={lssame}"))
    }
}
