use bridgevm_hvf::{
    fwcfg::GuestMemoryMut,
    msix::MsixMessage,
    pcie::{VIRTIO_GPU_MSIX_PBA_OFFSET, VIRTIO_GPU_MSIX_TABLE_OFFSET},
    platform_virt::FlatGuestRam,
    virtio_gpu::{VirtioGpuResult, VirtioPciGpu, VirtioPciGpuOp},
};

pub fn message(vector: u16) -> MsixMessage {
    MsixMessage {
        vector,
        address: 0xfee0_1000,
        data: 0x70 + u32::from(vector),
    }
}

pub struct Gpu {
    pub dev: VirtioPciGpu,
    mem: FlatGuestRam,
}

impl Gpu {
    pub fn new(config_vector: u16) -> Self {
        let mut gpu = Self {
            dev: VirtioPciGpu::new(1280, 800),
            mem: FlatGuestRam::new(0, 0x6000),
        };
        gpu.program(config_vector);
        gpu
    }

    pub fn write(&mut self, offset: u64, size: u8, value: u64) {
        assert_eq!(
            self.dev
                .access(offset, VirtioPciGpuOp::Write { size, value }, &mut self.mem),
            VirtioGpuResult::WriteAck
        );
    }

    pub fn read(&mut self, offset: u64, size: u8) -> u64 {
        match self
            .dev
            .access(offset, VirtioPciGpuOp::Read { size }, &mut self.mem)
        {
            VirtioGpuResult::ReadValue(value) => value,
            other => panic!("register read returned {other:?}"),
        }
    }

    pub fn program(&mut self, config_vector: u16) {
        self.write(0x10, 2, u64::from(config_vector));
        for vector in [0, 1] {
            let table = u64::from(VIRTIO_GPU_MSIX_TABLE_OFFSET) + u64::from(vector) * 16;
            let message = message(vector);
            self.dev.msix_bar_access(
                table,
                VirtioPciGpuOp::Write {
                    size: 8,
                    value: message.address,
                },
            );
            self.dev.msix_bar_access(
                table + 8,
                VirtioPciGpuOp::Write {
                    size: 4,
                    value: u64::from(message.data),
                },
            );
            self.mask(vector, false);
        }
    }

    pub fn mask(&mut self, vector: u16, masked: bool) {
        let offset = u64::from(VIRTIO_GPU_MSIX_TABLE_OFFSET) + u64::from(vector) * 16 + 12;
        assert_eq!(
            self.dev.msix_bar_access(
                offset,
                VirtioPciGpuOp::Write {
                    size: 4,
                    value: u64::from(masked)
                }
            ),
            VirtioGpuResult::WriteAck
        );
    }

    pub fn pba(&mut self) -> u64 {
        match self.dev.msix_bar_access(
            u64::from(VIRTIO_GPU_MSIX_PBA_OFFSET),
            VirtioPciGpuOp::Read { size: 8 },
        ) {
            VirtioGpuResult::ReadValue(value) => value,
            other => panic!("PBA read returned {other:?}"),
        }
    }

    pub fn complete_display_info(&mut self, vector: u16) {
        // Ordinary GET_DISPLAY_INFO through a readable header and writable response.
        for (address, bytes) in [
            (0x1000, 0x4000u64.to_le_bytes().to_vec()),
            (0x1008, 24u32.to_le_bytes().to_vec()),
            (0x100c, 1u16.to_le_bytes().to_vec()),
            (0x100e, 1u16.to_le_bytes().to_vec()),
            (0x1010, 0x5000u64.to_le_bytes().to_vec()),
            (0x1018, 408u32.to_le_bytes().to_vec()),
            (0x101c, 2u16.to_le_bytes().to_vec()),
            (0x2002, 1u16.to_le_bytes().to_vec()),
            (0x4000, 0x100u32.to_le_bytes().to_vec()),
        ] {
            assert!(self.mem.write_bytes(address, &bytes));
        }
        for (offset, size, value) in [
            (0x14, 1, 3),
            (0x08, 4, 1),
            (0x0c, 4, 1),
            (0x14, 1, 11),
            (0x16, 2, 0),
            (0x18, 2, 16),
            (0x1a, 2, u64::from(vector)),
            (0x20, 8, 0x1000),
            (0x28, 8, 0x2000),
            (0x30, 8, 0x3000),
            (0x1c, 2, 1),
            (0x14, 1, 15),
            (0x3000, 4, 0),
        ] {
            self.write(offset, size, value);
        }
        assert_eq!(self.mem.read_bytes(0x3002, 2).unwrap(), [1, 0]);
        assert_eq!(
            self.mem.read_bytes(0x5000, 4).unwrap(),
            0x1101u32.to_le_bytes()
        );
        assert!(self.dev.stats().queues[0].pending_msix);
    }
}
