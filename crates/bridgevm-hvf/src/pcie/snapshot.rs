//! Checkpoint save and restore of guest-programmed config-space state.

use super::*;

impl PcieEcam {
    pub fn snapshot_state(&self) -> Vec<u8> {
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_u32(self.functions.len() as u32);

        for function in &self.functions {
            out.write_u8(function.bdf.0);
            out.write_u8(function.bdf.1);
            out.write_u8(function.bdf.2);
            out.write_u8(0);
            out.write_u16(function.command);
            out.write_u16(0);

            for bar in &function.bars {
                out.write_u32(bar.value);
            }

            out.write_u32(function.cap_bytes.len() as u32);
            for &(offset, value) in &function.cap_bytes {
                out.write_u16(offset);
                out.write_u8(value);
                out.write_u8(0);
            }
        }

        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(input.read_u32(), 1, "unsupported PCIe snapshot version");
        assert_eq!(
            input.read_u32() as usize,
            self.functions.len(),
            "PCIe function-count mismatch on restore"
        );

        for function in &mut self.functions {
            let bdf = (input.read_u8(), input.read_u8(), input.read_u8());
            assert_eq!(input.read_u8(), 0, "invalid PCIe snapshot");
            assert_eq!(bdf, function.bdf, "PCIe BDF mismatch on restore");

            function.command = input.read_u16() & CMD_WRITABLE_MASK;
            assert_eq!(input.read_u16(), 0, "invalid PCIe snapshot");

            for bar in &mut function.bars {
                bar.value = input.read_u32();
            }

            let capability_count = input.read_u32() as usize;
            assert_eq!(
                capability_count,
                function.cap_bytes.len(),
                "PCIe capability shape mismatch on restore"
            );
            for capability in &mut function.cap_bytes {
                let offset = input.read_u16();
                let value = input.read_u8();
                assert_eq!(input.read_u8(), 0, "invalid PCIe snapshot");
                assert_eq!(offset, capability.0, "PCIe capability offset mismatch");
                capability.1 = value;
            }
        }

        self.mmio_mru.set(None);
        input.finish();
    }
}
